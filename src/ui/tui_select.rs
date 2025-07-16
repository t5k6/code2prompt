use std::collections::HashSet;
use std::io::{Stdout, stdout};
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, MouseButton,
        MouseEvent, MouseEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Clear, List, ListItem, ListState, Paragraph, Row, Table},
};
use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};
use thousands::Separable;

use crate::common::format::{self, TokenFormatStyle};
use crate::engine::token::TokenizerChoice;
use crate::ui::cache::LastSelection;
use crate::ui::pane::NavigablePane;
use crate::ui::tree_arena::{DirFlags, DirNode};
use crate::ui::tree_pane::TreePane;

/// Settings that can be modified in the TUI. Mirrors a subset of `Code2PromptConfig`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct TuiSettings {
    pub line_numbers: bool,
    pub hidden: bool,
    pub follow_symlinks: bool,
    pub no_codeblock: bool,
    pub tokenizer: TokenizerChoice,
}

#[derive(Clone, Copy, Debug)]
enum SettingFlag {
    LineNumbers,
    Hidden,
    FollowSymlinks,
    NoCodeblock,
    Tokenizer,
}

impl SettingFlag {
    const ALL: [SettingFlag; 5] = [
        // Update the count and content
        SettingFlag::LineNumbers,
        SettingFlag::Hidden,
        SettingFlag::FollowSymlinks,
        SettingFlag::NoCodeblock,
        SettingFlag::Tokenizer,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::LineNumbers => "Show line numbers",
            Self::Hidden => "Include hidden files",
            Self::FollowSymlinks => "Follow symlinks",
            Self::NoCodeblock => "Disable ``` code blocks",
            Self::Tokenizer => "Tokenizer",
        }
    }

    /// Generates the full display line for the settings menu.
    fn display_line(&self, s: &TuiSettings, is_selected: bool) -> Line<'static> {
        match self {
            Self::LineNumbers | Self::Hidden | Self::FollowSymlinks | Self::NoCodeblock => {
                let is_enabled = match self {
                    Self::LineNumbers => s.line_numbers,
                    Self::Hidden => s.hidden,
                    Self::FollowSymlinks => s.follow_symlinks,
                    Self::NoCodeblock => s.no_codeblock,
                    _ => unreachable!(),
                };
                let mark = if is_enabled { 'x' } else { ' ' };
                Line::from(format!("[{mark}] {}", self.label()))
            }
            Self::Tokenizer => {
                let value_style = if is_selected {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };

                Line::from(vec![
                    Span::raw(format!("{:<25}: ", self.label())),
                    Span::styled(format!("< {} >", s.tokenizer), value_style),
                ])
            }
        }
    }

    /// Cycles to the next value for the setting.
    fn cycle_next(&self, s: &mut TuiSettings) {
        match self {
            Self::LineNumbers => s.line_numbers = !s.line_numbers,
            Self::Hidden => s.hidden = !s.hidden,
            Self::FollowSymlinks => s.follow_symlinks = !s.follow_symlinks,
            Self::NoCodeblock => s.no_codeblock = !s.no_codeblock,
            Self::Tokenizer => s.tokenizer = s.tokenizer.next(),
        }
    }

    fn cycle_previous(&self, s: &mut TuiSettings) {
        match self {
            // Booleans just toggle, so it's the same as cycle_next
            Self::LineNumbers | Self::Hidden | Self::FollowSymlinks | Self::NoCodeblock => {
                self.cycle_next(s)
            }
            // For the tokenizer, we call the new `previous` method
            Self::Tokenizer => s.tokenizer = s.tokenizer.previous(),
        }
    }
}

/// Defines the possible outcomes of the TUI selection process.
pub enum TuiAction {
    /// User confirmed their file/directory selections.
    Confirm {
        exts: Vec<String>,
        paths: Vec<PathBuf>,
    },
    /// User requested to quit the application.
    Cancel,
    /// User changed settings and wants to re-scan the codebase.
    RescanWithConfig {
        settings: TuiSettings,
        show_msg: bool,
    },
}

struct TerminalGuard(Terminal<CrosstermBackend<Stdout>>);

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        // We can mutably borrow self.0 because the guard owns it.
        let _ = restore_terminal(&mut self.0);
    }
}

// Help text constant
const HELP_TEXT: &str =
    "Tab: Switch panes | Space: Toggle | s: Settings | Enter: Confirm | q/Esc: Quit | /: Filter";

// Application input mode
pub(crate) enum AppMode {
    Normal,
    Filtering,
    Settings,
}

/// A helper to create a styled block for a TUI pane, now simpler without title.
fn pane_block(active: bool) -> Block<'static> {
    let border_style = if active {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
}

pub(crate) struct ListPane<T>
where
    T: Clone,
{
   pub(crate) items: Vec<T>,
   pub(crate) selected: Vec<bool>,
   pub(crate) state: ListState,
   pub(crate) filter: String,
   pub(crate) filtered_indices: Vec<usize>,
}

impl<T: Clone> NavigablePane for ListPane<T> {
    fn next(&mut self) {
        self.next();
    }

    fn previous(&mut self) {
        self.previous();
    }

    fn toggle_current_selection(&mut self) -> bool {
        self.toggle_selection()
    }
}

fn find_substring_case_insensitive(hay: &[u8], pat: &[u8]) -> bool {
    if pat.is_empty() {
        return true;
    }
    hay.windows(pat.len()).any(|w| w.eq_ignore_ascii_case(pat))
}

impl<T> ListPane<T>
where
    T: Clone,
{
    fn new(
        items: Vec<T>,
        initial_selection_keys: Option<&[String]>,
        item_to_string_fn: impl Fn(&T) -> &str,
    ) -> Self {
        let mut state = ListState::default();
        let item_count = items.len();
        if !items.is_empty() {
            state.select(Some(0));
        }
        let mut selected = vec![false; item_count];
        if let Some(keys) = initial_selection_keys {
            let key_set: HashSet<_> = keys.iter().map(|s| s.as_str()).collect();
            for (i, item) in items.iter().enumerate() {
                if key_set.contains(item_to_string_fn(item)) {
                    selected[i] = true;
                }
            }
        } else {
            selected.fill(true);
        }
        Self {
            items,
            selected,
            state,
            filter: String::new(),
            filtered_indices: (0..item_count).collect(),
        }
    }

    fn apply_filter(&mut self, item_to_string_fn: impl Fn(&T) -> &str) {
        if self.filter.is_empty() {
            self.filtered_indices = (0..self.items.len()).collect();
        } else {
            let f = self.filter.as_bytes();
            self.filtered_indices = self
                .items
                .iter()
                .enumerate()
                .filter(|(_, item)| {
                    let s = item_to_string_fn(item).as_bytes();
                    find_substring_case_insensitive(s, f)
                })
                .map(|(i, _)| i)
                .collect();
        }
        if !self.filtered_indices.is_empty() {
            self.state.select(Some(0));
        } else {
            self.state.select(None);
        }
    }

    fn get_real_selected_index(&self) -> Option<usize> {
        self.state
            .selected()
            .and_then(|selected_in_filtered| self.filtered_indices.get(selected_in_filtered))
            .copied()
    }

    /// Toggles the selection of the currently highlighted item.
    /// Returns `true` if the selection state was changed.
    pub fn toggle_selection(&mut self) -> bool {
        if let Some(i) = self.get_real_selected_index() {
            self.selected[i] = !self.selected[i];
            return true;
        }
        false
    }

    /// Selects all items currently visible in the filtered list.
    /// Returns `true` if the selection state was changed.
    fn select_all(&mut self) -> bool {
        let mut changed = false;
        for &index in &self.filtered_indices {
            if !self.selected[index] {
                self.selected[index] = true;
                changed = true;
            }
        }
        changed
    }

    /// Deselects all items currently visible in the filtered list.
    /// Returns `true` if the selection state was changed.
    fn deselect_all(&mut self) -> bool {
        let mut changed = false;
        for &index in &self.filtered_indices {
            if self.selected[index] {
                self.selected[index] = false;
                changed = true;
            }
        }
        changed
    }

    /// Inverts the selection of all items currently visible in the filtered list.
    /// Returns `true` if the selection state was changed (and the list is not empty).
    fn invert_selection(&mut self) -> bool {
        if self.filtered_indices.is_empty() {
            return false;
        }
        for &i in &self.filtered_indices {
            self.selected[i] = !self.selected[i];
        }
        true
    }

    fn next(&mut self) {
        if self.filtered_indices.is_empty() {
            return;
        }
        let i = match self.state.selected() {
            Some(i) => (i + 1) % self.filtered_indices.len(),
            None => 0,
        };
        self.state.select(Some(i));
    }

    fn previous(&mut self) {
        if self.filtered_indices.is_empty() {
            return;
        }
        let i = match self.state.selected() {
            Some(i) => {
                if i == 0 {
                    self.filtered_indices.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }
}

pub(crate) struct App {
    pub repo_name: String,
    pub extensions: ListPane<(String, usize)>,
    pub directories: TreePane,
    pub active_pane: Pane,
    pub mode: AppMode,
    pub active_exts: FxHashSet<String>,
    pub total_selected_files: usize,
    pub list_render_buffer: Vec<ListItem<'static>>,
    pub ext_to_slot: FxHashMap<String, u16>,
    pub ext_totals: Vec<usize>,
    pub settings: TuiSettings,
    pub settings_state: ListState,
}

enum DfsState {
    Descend, // First time seeing a node, push children to stack.
    Ascend,  // Second time seeing a node, after children are processed.
}

impl App {
    /// Returns a mutable trait object for the currently active pane.
    /// This is the key to unifying event handling via dynamic dispatch.
    fn active_pane(&mut self) -> &mut dyn NavigablePane {
        match self.active_pane {
            Pane::Extensions => &mut self.extensions,
            Pane::Directories => &mut self.directories,
        }
    }

    fn ext_pane(&self) -> &ListPane<(String, usize)> {
        &self.extensions
    }

    fn switch_pane(&mut self) {
        // Only the extensions pane has a text filter to clear.
        if self.active_pane == Pane::Extensions {
            self.extensions.filter.clear();
            self.apply_active_filter();
        }
        self.active_pane = match self.active_pane {
            Pane::Extensions => Pane::Directories,
            Pane::Directories => Pane::Extensions,
        };
    }

    fn apply_active_filter(&mut self) {
        let item_to_string_fn: fn(&(String, usize)) -> &str = |item| &item.0;
        self.extensions.apply_filter(item_to_string_fn);
    }

    fn enter_filtering_mode(&mut self) {
        self.mode = AppMode::Filtering;
    }

    fn exit_filtering_mode(&mut self) {
        self.mode = AppMode::Normal;
        self.recalculate_all_visible_counts();
    }

    fn cancel_filtering(&mut self) {
        self.extensions.filter.clear();
        self.apply_active_filter();
        self.mode = AppMode::Normal;
        self.recalculate_all_visible_counts();
    }

    fn recalculate_all_visible_counts(&mut self) {
        // 1. Rebuild active extensions set
        self.active_exts.clear();
        for (i, (ext, _)) in self.extensions.items.iter().enumerate() {
            if self.extensions.selected[i] {
                self.active_exts.insert(ext.clone());
            }
        }

        // 2. Zero out totals
        self.ext_totals.fill(0);

        let arena = &mut self.directories.arena;
        // The stack holds: (node_index, traversal_state, is_ancestor_selected)
        let mut stack = vec![(0, DfsState::Descend, false)];

        // 3. Single-pass iterative DFS
        while let Some((idx, state, ancestor_selected)) = stack.pop() {
            let is_node_selected = arena[idx as usize].flags.contains(DirFlags::SELECTED);
            let effective_selection = ancestor_selected || is_node_selected;

            match state {
                DfsState::Descend => {
                    // We're going down. Mark for ascent and push children.
                    stack.push((idx, DfsState::Ascend, ancestor_selected));

                    let node = &arena[idx as usize];
                    if node.flags.contains(DirFlags::IS_DIR) {
                        let mut child_opt = node.first_child;
                        while let Some(child_idx) = child_opt {
                            stack.push((child_idx, DfsState::Descend, effective_selection));
                            child_opt = arena[child_idx as usize].next_sibling;
                        }
                    }
                }
                DfsState::Ascend => {
                    // We're going up. Children have been processed. Calculate this node's totals.

                    // First, calculate the sums from the children without holding a mutable borrow
                    // on the parent.
                    let mut children_visible_files = 0;
                    let mut children_visible_toks = 0;

                    if arena[idx as usize].flags.contains(DirFlags::IS_DIR) {
                        let mut child_opt = arena[idx as usize].first_child;
                        while let Some(child_idx) = child_opt {
                            let child_node = &arena[child_idx as usize]; // This is now a safe immutable borrow
                            children_visible_files += child_node.visible_files;
                            children_visible_toks += child_node.visible_toks;
                            child_opt = child_node.next_sibling;
                        }
                    }

                    // Now, get the mutable borrow of the current node and update it.
                    // The borrows on the children from the loop above are now out of scope.
                    let node = &mut arena[idx as usize];
                    node.visible_files = children_visible_files;
                    node.visible_toks = children_visible_toks;

                    if !node.flags.contains(DirFlags::IS_DIR) {
                        // It's a file. Handle its own contribution.
                        if effective_selection {
                            if self
                                .active_exts
                                .contains(node.extension.as_deref().unwrap_or_default())
                            {
                                node.visible_files = 1;
                                node.visible_toks = node.total_toks;
                                if node.ext_slot != 0 {
                                    self.ext_totals[node.ext_slot as usize] += node.total_toks;
                                }
                            }
                        }
                    }
                }
            }
        }

        // 4. Update total counts from the root node
        self.total_selected_files = self.directories.arena[0].visible_files;

        // 5. Write back extension totals to the ListPane's data
        for (ext, count) in &mut self.extensions.items {
            if let Some(slot) = self.ext_to_slot.get(ext) {
                *count = self.ext_totals[*slot as usize];
            }
        }

        // 6. Rebuild the visible node list for rendering
        self.directories.rebuild_visible(&self.active_exts);
    }
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub(crate) enum Pane {
    Extensions,
    Directories,
}

pub fn select_filters_tui(
    repo_path: &std::path::Path,
    extensions: Vec<(String, usize)>,
    dir_arena: Vec<DirNode>,
    last_selection: Option<LastSelection>,
    initial_config: &crate::engine::config::Code2PromptConfig,
) -> Result<TuiAction> {
    // 1. Setup terminal and immediately pass ownership to the guard.
    let terminal = setup_terminal()?;
    let mut guard = TerminalGuard(terminal);

    drain_input_buffer()?;

    let mut ext_to_slot: FxHashMap<String, u16> = FxHashMap::default();
    for (i, (ext, _)) in extensions.iter().enumerate() {
        ext_to_slot.insert(ext.clone(), (i + 1) as u16);
    }
    let ext_count = extensions.len();

    let initial_settings = TuiSettings {
        line_numbers: initial_config.line_numbers,
        hidden: initial_config.hidden,
        follow_symlinks: initial_config.follow_symlinks,
        no_codeblock: initial_config.no_codeblock,
        tokenizer: initial_config.tokenizer,
    };

    let mut app = App {
        repo_name: repo_path
            .file_name()
            .unwrap_or(repo_path.as_os_str())
            .to_string_lossy()
            .to_string(),
        extensions: ListPane::new(
            extensions,
            last_selection.as_ref().map(|s| s.extensions.as_slice()),
            |item| &item.0,
        ),
        directories: TreePane::new(dir_arena, last_selection.as_ref()),
        active_pane: Pane::Extensions,
        mode: AppMode::Normal,
        total_selected_files: 0,
        list_render_buffer: Vec::new(),
        active_exts: FxHashSet::default(),
        ext_to_slot,
        ext_totals: vec![0; ext_count + 1],
        settings: initial_settings,
        settings_state: ListState::default(),
    };

    app.recalculate_all_visible_counts();

    if app.extensions.items.is_empty() && !app.directories.arena.is_empty() {
        app.active_pane = Pane::Directories;
    }

    // 2. Pass a mutable borrow of the terminal *from the guard's field* to the loop.
    let action = run_event_loop(&mut guard.0, &mut app)?;

    match action {
        TuiAction::Confirm { .. } => {
            let chosen_ext = app
                .extensions
                .items
                .iter()
                .zip(&app.extensions.selected)
                .filter_map(|((e, _), sel)| sel.then(|| e.clone()))
                .collect();
            let chosen_dir = app.directories.get_selected_paths();
            Ok(TuiAction::Confirm {
                exts: chosen_ext,
                paths: chosen_dir,
            })
        }
        other_action => Ok(other_action),
    }
}

fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut App,
) -> Result<TuiAction> {
    loop {
        terminal.draw(|f| ui(f, app))?;
        if event::poll(Duration::from_millis(250))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => match app.mode {
                    AppMode::Normal => match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => return Ok(TuiAction::Cancel),
                        KeyCode::Enter => {
                            return Ok(TuiAction::Confirm {
                                exts: vec![],
                                paths: vec![],
                            });
                        }
                        KeyCode::Char('/') => {
                            if app.active_pane == Pane::Extensions {
                                app.enter_filtering_mode();
                            }
                        }
                        _ => {
                            if let Some(action) = handle_key_press_normal(app, key.code) {
                                return Ok(action);
                            }
                        }
                    },
                    AppMode::Filtering => match key.code {
                        KeyCode::Enter => app.exit_filtering_mode(),
                        KeyCode::Esc => app.cancel_filtering(),
                        _ => handle_key_press_filtering(app, key.code),
                    },
                    AppMode::Settings => {
                        if let Some(action) = handle_key_press_settings(app, key.code) {
                            return Ok(action);
                        }
                    }
                },
                Event::Mouse(mouse_event) => handle_mouse_event(app, mouse_event),
                _ => {}
            }
        }
    }
}

fn handle_mouse_event(app: &mut App, mouse_event: MouseEvent) {
    match mouse_event.kind {
        MouseEventKind::ScrollUp => app.active_pane().previous(),
        MouseEventKind::ScrollDown => app.active_pane().next(),
        MouseEventKind::Down(MouseButton::Left) => {
            handle_mouse_click(app, mouse_event.row, mouse_event.column);
        }
        _ => {}
    }
}

pub(crate) fn handle_mouse_click(app: &mut App, row: u16, _column: u16) {
    let clicked_row = row as usize;

    // Must be below header (row 0 = title, row 1 = border top)
    if clicked_row <= 1 {
        return;
    }

    match app.active_pane {
        Pane::Extensions => {
            // Since we can't access offset directly, we'll use a simpler approach
            // Just select the item at the clicked position relative to visible area
            let list_index = clicked_row.saturating_sub(2); // Adjust for header

            if list_index < app.extensions.filtered_indices.len() {
                app.extensions.state.select(Some(list_index));
                app.extensions.toggle_selection();
                app.recalculate_all_visible_counts();
            }
        }
        Pane::Directories => {
            // Since we can't access offset directly, we'll use a simpler approach
            // Just select the item at the clicked position relative to visible area
            let list_index = clicked_row.saturating_sub(2); // Adjust for header

            if list_index < app.directories.visible_nodes.len() {
                app.directories.cursor = list_index;
                app.directories
                    .list_state
                    .select(Some(app.directories.cursor));
                app.directories.toggle_selection();
                app.recalculate_all_visible_counts();
            }
        }
    }
}

fn handle_key_press_normal(app: &mut App, key_code: KeyCode) -> Option<TuiAction> {
    let mut needs_recalc = false;
    let mut needs_rebuild_visible = false;

    match key_code {
        KeyCode::Up | KeyCode::Char('k') => app.active_pane().previous(),
        KeyCode::Down | KeyCode::Char('j') => app.active_pane().next(),
        KeyCode::Char(' ') => needs_recalc = app.active_pane().toggle_current_selection(),
        KeyCode::Tab => app.switch_pane(),
        KeyCode::Char('s') => {
            app.mode = AppMode::Settings;
            app.settings_state.select(Some(0));
        }
        _ => match app.active_pane {
            Pane::Extensions => match key_code {
                KeyCode::Char('a') => needs_recalc = app.extensions.select_all(),
                KeyCode::Char('n') => needs_recalc = app.extensions.deselect_all(),
                KeyCode::Char('i') => needs_recalc = app.extensions.invert_selection(),
                KeyCode::Right | KeyCode::Char('l') => app.switch_pane(),
                _ => {}
            },
            Pane::Directories => match key_code {
                KeyCode::Right | KeyCode::Char('l') => {
                    app.directories.toggle_expand();
                    needs_rebuild_visible = true;
                }
                KeyCode::Left | KeyCode::Char('h') => {
                    if let Some(idx) = app.directories.get_current_node_idx() {
                        let node = &app.directories.arena[idx as usize];
                        if node.flags.contains(DirFlags::IS_DIR | DirFlags::EXPANDED) {
                            app.directories.collapse_or_move_to_parent();
                            needs_rebuild_visible = true;
                        } else if node.parent.is_some() && node.parent != Some(0) {
                            app.directories.collapse_or_move_to_parent();
                        } else {
                            app.switch_pane();
                        }
                    } else {
                        app.switch_pane();
                    }
                }
                _ => {}
            },
        },
    }

    if needs_recalc {
        app.recalculate_all_visible_counts();
    }
    if needs_rebuild_visible {
        app.directories.rebuild_visible(&app.active_exts);
    }

    // Explicitly return None, signifying no final action was taken here.
    None
}

fn handle_key_press_filtering(app: &mut App, key_code: KeyCode) {
    // This only applies to the Extensions pane, so we don't use the trait here.
    if app.active_pane == Pane::Extensions {
        let pane = &mut app.extensions;
        match key_code {
            KeyCode::Up | KeyCode::Char('k') => pane.previous(),
            KeyCode::Down | KeyCode::Char('j') => pane.next(),
            KeyCode::Char(' ') => {
                let _ = pane.toggle_selection();
            }
            KeyCode::Char(c) => {
                pane.filter.push(c);
                app.apply_active_filter();
            }
            KeyCode::Backspace => {
                pane.filter.pop();
                app.apply_active_filter();
            }
            _ => {}
        }
    }
}

fn ui(f: &mut Frame, app: &mut App) {
    let size = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(size);
    f.render_widget(
        Paragraph::new(format!("code2prompt ▸ {}", app.repo_name))
            .style(Style::default().add_modifier(Modifier::BOLD)),
        chunks[0],
    );
    let footer_text: Line = match app.mode {
        AppMode::Normal => {
            let active_style = Style::default().fg(Color::Black).bg(Color::Yellow);
            let inactive_style = Style::default().fg(Color::DarkGray).bg(Color::Black);

            let ext_style = if app.active_pane == Pane::Extensions {
                active_style
            } else {
                inactive_style
            };
            let dir_style = if app.active_pane == Pane::Directories {
                active_style
            } else {
                inactive_style
            };

            let total_selected_files = app.total_selected_files;
            let total_files = app.directories.arena[0].file_count;
            let ext_count = app.extensions.selected.iter().filter(|&&x| x).count();
            let ext_total = app.extensions.items.len();

            Line::from(vec![
                Span::raw(HELP_TEXT),
                Span::raw("  "),
                Span::styled(format!(" Ext: {ext_count}/{ext_total} "), ext_style),
                Span::raw(" "),
                Span::styled(
                    format!(" Files: {total_selected_files}/{total_files} "),
                    dir_style,
                ),
            ])
        }
        AppMode::Filtering => {
            let count = if app.active_pane == Pane::Extensions {
                app.ext_pane().filtered_indices.len()
            } else {
                0
            };
            Line::from(vec![
                Span::raw("FILTER: "),
                Span::styled(&app.ext_pane().filter, Style::default().fg(Color::Yellow)),
                Span::raw(format!(
                    " | Matches: {} (Esc to Cancel, Enter to Confirm)",
                    count
                )),
            ])
        }
        AppMode::Settings => Line::from(vec![
            Span::raw("SETTINGS"),
            Span::raw(" | "),
            Span::styled(
                "Up/Down: Navigate | Space/Arrows: Change Value | Enter: Apply | Esc: Cancel", // <-- UPDATED TEXT
                Style::default().fg(Color::Yellow),
            ),
        ]),
    };
    f.render_widget(
        Paragraph::new(footer_text).style(Style::default().fg(Color::DarkGray)),
        chunks[2],
    );
    let content_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(chunks[1]);

    // --- Extensions Pane ---
    app.list_render_buffer.clear();
    app.extensions
        .filtered_indices
        .iter()
        .for_each(|&real_index| {
            let item = &app.extensions.items[real_index];
            let is_selected = app.extensions.selected[real_index];
            let (ext, tokens) = item;
            let mark = if is_selected { "●" } else { "○" };
            let toks = format::format_tokens(*tokens, TokenFormatStyle::Compact);
            let line = format!("{mark} {ext:<8} {toks:>6}");
            app.list_render_buffer.push(ListItem::new(line));
        });

    let is_dir_active = app.active_pane == Pane::Directories;
    let ext_list = create_styled_list(
        "File Types",
        !is_dir_active,
        std::mem::take(&mut app.list_render_buffer),
    );
    f.render_stateful_widget(ext_list, content_chunks[0], &mut app.extensions.state);

    // ---------- Tree Pane ----------
    // Build rows for Table widget (3 columns)
    let mut rows: Vec<Row> = Vec::with_capacity(app.directories.visible_nodes.len());
    for &idx in &app.directories.visible_nodes {
        let n = &app.directories.arena[idx as usize];
        let depth = app.directories.get_depth(idx);
        let indent = " ".repeat(depth);

        // icons + tick mark
        let tri = if n.flags.contains(DirFlags::IS_DIR) {
            if n.flags.contains(DirFlags::EXPANDED) {
                "▾"
            } else {
                "▸"
            }
        } else {
            " "
        };
        let tick = match (
            n.flags.contains(DirFlags::SELECTED),
            app.directories.has_partial_selection(idx),
        ) {
            (true, _) => "●",
            (false, true) => "◐",
            _ => "○",
        };
        let name_cell = Cell::from(format!("{indent}{tri} {tick} {}", n.name));

        // files column
        let files_txt = n.file_count.separate_with_dots();
        let files_cell = if n.file_count == 0 {
            Cell::from(files_txt).style(Style::default().fg(Color::DarkGray))
        } else {
            Cell::from(files_txt)
        };

        // tokens column
        let toks_txt = format::format_tokens(n.visible_toks, TokenFormatStyle::Compact);
        let toks_cell = if n.visible_toks == 0 {
            Cell::from(toks_txt).style(Style::default().fg(Color::DarkGray))
        } else {
            Cell::from(toks_txt)
        };

        rows.push(Row::new(vec![name_cell, files_cell, toks_cell]));
    }

    // column widths: name flexible, numbers fixed
    let widths = [
        Constraint::Percentage(70),
        Constraint::Length(7),
        Constraint::Length(7),
    ];

    let table = Table::new(rows, widths)
        .header(
            Row::new(vec!["Name", "Files", "Tokens"])
                .style(Style::default().add_modifier(Modifier::BOLD)),
        )
        .block(pane_block(is_dir_active).title(" Files & Folders "))
        .row_highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(">> ");
    f.render_stateful_widget(table, content_chunks[1], &mut app.directories.list_state);

    if matches!(app.mode, AppMode::Settings) {
        render_settings_popup(f, app);
    }
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    let mut stdout = stdout();
    enable_raw_mode()?;
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    Ok(Terminal::new(CrosstermBackend::new(stdout))?)
}

fn drain_input_buffer() -> Result<()> {
    while event::poll(Duration::from_millis(0))? {
        let _ = event::read()?;
    }
    Ok(())
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}

fn create_styled_list<'a>(title: &'a str, is_active: bool, items: Vec<ListItem<'a>>) -> List<'a> {
    let title_style = if is_active {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    let block = pane_block(is_active).title(Span::styled(format!(" {title} "), title_style));
    List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(">> ")
}

fn handle_key_press_settings(app: &mut App, key_code: KeyCode) -> Option<TuiAction> {
    let len = SettingFlag::ALL.len();
    if let Some(idx) = app.settings_state.selected() {
        let selected_flag = SettingFlag::ALL[idx];

        match key_code {
            KeyCode::Esc => {
                app.mode = AppMode::Normal;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let i = app.settings_state.selected().unwrap_or(0);
                app.settings_state.select(Some((i + len - 1) % len));
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let i = app.settings_state.selected().unwrap_or(0);
                // The logic is now driven by the length of the ALL array
                app.settings_state
                    .select(Some((i + 1) % SettingFlag::ALL.len()));
            }
            KeyCode::Char(' ') | KeyCode::Right | KeyCode::Char('l') => {
                selected_flag.cycle_next(&mut app.settings);
            }
            KeyCode::Left | KeyCode::Char('h') => {
                selected_flag.cycle_previous(&mut app.settings);
            }
            KeyCode::Enter => {
                return Some(TuiAction::RescanWithConfig {
                    settings: app.settings.clone(),
                    show_msg: true,
                });
            }
            _ => {}
        }
    } else if key_code == KeyCode::Esc {
        app.mode = AppMode::Normal;
    }
    None
}

fn render_settings_popup(f: &mut Frame, app: &mut App) {
    let items: Vec<_> = SettingFlag::ALL
        .iter()
        .enumerate() // We need the index to check if it's selected
        .map(|(i, flag)| {
            let is_selected = app.settings_state.selected() == Some(i);
            // Instead of just a string, we now create a Line with styled Spans
            ListItem::new(flag.display_line(&app.settings, is_selected))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Settings (Enter to Apply, Esc to Cancel) "),
        )
        .highlight_symbol(">> ")
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .bg(Color::DarkGray),
        );

    let area = centered_rect(60, 40, f.area());
    f.render_widget(Clear, area); // This clears the area under the popup
    f.render_stateful_widget(list, area, &mut app.settings_state);
}

/// Helper to create a centered rectangle for popups.
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
