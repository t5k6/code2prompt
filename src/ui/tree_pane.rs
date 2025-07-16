use std::path::PathBuf;

use ratatui::widgets::TableState;
use rustc_hash::FxHashSet;

use crate::ui::{
    cache::LastSelection,
    pane::NavigablePane,
    tree_arena::{DirFlags, DirNode, Idx},
};

#[derive(Debug)]
pub struct TreePane {
    pub arena: Vec<DirNode>,
    pub visible_nodes: Vec<Idx>,
    pub allowed_nodes: FxHashSet<Idx>,
    pub cursor: usize, // Index into visible_nodes
    pub list_state: TableState,
    pub last_filter: FxHashSet<String>,
}

impl NavigablePane for TreePane {
    fn next(&mut self) {
        self.next();
    }

    fn previous(&mut self) {
        self.previous();
    }

    fn toggle_current_selection(&mut self) -> bool {
        self.toggle_selection();
        // A selection change in the directory tree *always* requires a recalculation
        // of visible tokens and files.
        true
    }
}

impl TreePane {
    pub fn new(mut arena: Vec<DirNode>, last_selection: Option<&LastSelection>) -> Self {
        if let Some(selection) = last_selection {
            if !selection.directories.is_empty() {
                let key_set: std::collections::HashSet<_> = selection.directories.iter().collect();

                // 1. Unselect everything to ensure a clean slate from the cache.
                for node in &mut arena {
                    node.flags.remove(DirFlags::SELECTED);
                }

                // 2. Identify all nodes that match a path from the cache.
                let mut nodes_to_select = Vec::new();
                for i in 1..arena.len() {
                    let node_path = Self::get_path(&arena, i as Idx);
                    if key_set.contains(&node_path) {
                        nodes_to_select.push(i as Idx);
                    }
                }

                // 3. For each matched node, apply the full, correct selection logic.
                for node_idx in nodes_to_select {
                    // This will handle recursion down to children.
                    Self::set_selection_recursive(&mut arena, node_idx, true);

                    // This will correctly update the parent states (e.g., to partial or full).
                    let mut current_ancestor = arena[node_idx as usize].parent;
                    while let Some(parent_idx) = current_ancestor {
                        if parent_idx == 0 {
                            break;
                        } // Don't update root's parent
                        Self::update_parent_selection_state(&mut arena, parent_idx);
                        current_ancestor = arena[parent_idx as usize].parent;
                    }
                }
            }
        }

        let mut pane = Self {
            arena,
            visible_nodes: Vec::new(),
            allowed_nodes: FxHashSet::default(),
            cursor: 0,
            list_state: TableState::default(),
            last_filter: FxHashSet::default(),
        };

        if !pane.visible_nodes.is_empty() {
            pane.list_state.select(Some(0));
        }
        pane
    }

    /// Rebuilds the `visible_nodes` vector based on the `expanded` state of nodes.
    pub fn rebuild_visible(&mut self, active_extensions: &FxHashSet<String>) {
        let old_cursor_id = self.visible_nodes.get(self.cursor).copied();
        self.visible_nodes.clear();

        if active_extensions.is_empty() {
            self.cursor = 0;
            self.list_state.select(Some(0));
            return;
        }

        // --- Pass 1: Mark all nodes that should be part of the filtered tree ---
        // This is the expensive part. Only run it if the extension filter has changed.
        if &self.last_filter != active_extensions {
            self.allowed_nodes.clear();
            for i in 1..self.arena.len() {
                let node = &self.arena[i];
                if !node.flags.contains(DirFlags::IS_DIR) {
                    // It's a file
                    if node
                        .extension
                        .as_ref()
                        .is_some_and(|ext| active_extensions.contains(ext))
                    {
                        // Mark this file and all its ancestors as allowed
                        let mut current_idx = Some(i as Idx);
                        while let Some(idx) = current_idx {
                            self.allowed_nodes.insert(idx);
                            current_idx = self.arena[idx as usize].parent;
                        }
                    }
                }
            }
            self.last_filter = active_extensions.clone();
        }

        // --- Pass 2: Traverse the full tree structure and add nodes if they are allowed AND visible (due to expansion) ---
        let mut current_child_opt = self.arena[0].first_child;
        while let Some(child_idx) = current_child_opt {
            // We only need to start a walk if the top-level directory is allowed
            if self.allowed_nodes.contains(&child_idx) {
                self.walk_and_add(child_idx);
            }
            current_child_opt = self.arena[child_idx as usize].next_sibling;
        }

        // --- Restore cursor ---
        if let Some(id) = old_cursor_id {
            self.cursor = self
                .visible_nodes
                .iter()
                .position(|&idx| idx == id)
                .unwrap_or(0);
        }
        self.cursor = self.cursor.min(self.visible_nodes.len().saturating_sub(1));
        self.list_state.select(Some(self.cursor));
    }

    /// A recursive helper for pre-order traversal. Adds allowed nodes to the visible list.
    fn walk_and_add(&mut self, node_idx: Idx) {
        // Add the current node to the visible list. The check to start the walk
        // ensures it's an allowed node.
        self.visible_nodes.push(node_idx);

        let node = &self.arena[node_idx as usize];

        // If the node is an expanded directory, recurse into its children.
        if node.flags.contains(DirFlags::IS_DIR | DirFlags::EXPANDED) {
            let mut current_child_opt = node.first_child;
            while let Some(child_idx) = current_child_opt {
                // IMPORTANT: Only descend into children that are part of the filtered set.
                // This prevents showing empty branches of an expanded directory.
                if self.allowed_nodes.contains(&child_idx) {
                    self.walk_and_add(child_idx);
                }
                current_child_opt = self.arena[child_idx as usize].next_sibling;
            }
        }
    }

    /// A helper to get the depth of a node for indentation.
    pub fn get_depth(&self, node_idx: Idx) -> usize {
        let mut depth = 0;
        let mut current_parent_opt = self.arena[node_idx as usize].parent;
        while let Some(parent_idx) = current_parent_opt {
            // Stop when we reach the synthetic root's direct children
            if parent_idx == 0 {
                break;
            }
            depth += 1;
            current_parent_opt = self.arena[parent_idx as usize].parent;
        }
        depth
    }

    pub fn next(&mut self) {
        if self.visible_nodes.is_empty() {
            return;
        }
        let i = if self.cursor >= self.visible_nodes.len() - 1 {
            0
        } else {
            self.cursor + 1
        };
        self.cursor = i;
        self.list_state.select(Some(self.cursor));
    }

    pub fn previous(&mut self) {
        if self.visible_nodes.is_empty() {
            return;
        }
        let i = if self.cursor == 0 {
            self.visible_nodes.len() - 1
        } else {
            self.cursor - 1
        };
        self.cursor = i;
        self.list_state.select(Some(self.cursor));
    }

    pub fn toggle_expand(&mut self) {
        if let Some(&node_idx) = self.visible_nodes.get(self.cursor) {
            // This check is fine, but the toggle inside needs fixing
            if self.arena[node_idx as usize]
                .flags
                .contains(DirFlags::IS_DIR)
            {
                self.arena[node_idx as usize]
                    .flags
                    .toggle(DirFlags::EXPANDED);
            }
        }
    }

    pub fn collapse_or_move_to_parent(&mut self) {
        if let Some(&node_idx) = self.visible_nodes.get(self.cursor) {
            let node = &mut self.arena[node_idx as usize];
            if node.flags.contains(DirFlags::IS_DIR | DirFlags::EXPANDED) {
                node.flags.remove(DirFlags::EXPANDED); // Collapse the node
            } else if let Some(parent_idx) = node.parent {
                if parent_idx != 0 {
                    // Don't move to the synthetic root
                    if let Some(new_pos) =
                        self.visible_nodes.iter().position(|&idx| idx == parent_idx)
                    {
                        self.cursor = new_pos;
                        self.list_state.select(Some(new_pos));
                    }
                }
            }
        }
    }

    pub fn get_current_node_idx(&self) -> Option<Idx> {
        self.visible_nodes.get(self.cursor).copied()
    }

    // This method is now called by the trait implementation.
    // It's kept public in case it's needed elsewhere, but could be made private.
    pub fn toggle_selection(&mut self) {
        if let Some(node_idx) = self.get_current_node_idx() {
            let is_selected = self.arena[node_idx as usize]
                .flags
                .contains(DirFlags::SELECTED);
            Self::set_selection_recursive(&mut self.arena, node_idx, !is_selected);

            let mut current_ancestor = self.arena[node_idx as usize].parent;
            while let Some(parent_idx) = current_ancestor {
                if parent_idx == 0 {
                    break;
                }
                Self::update_parent_selection_state(&mut self.arena, parent_idx);
                current_ancestor = self.arena[parent_idx as usize].parent;
            }
        }
    }

    // This is now a static method that operates on the arena directly.
    fn set_selection_recursive(arena: &mut Vec<DirNode>, node_idx: Idx, select: bool) {
        let node_flags = &mut arena[node_idx as usize].flags;
        if select {
            node_flags.insert(DirFlags::SELECTED);
        } else {
            node_flags.remove(DirFlags::SELECTED);
        }

        if arena[node_idx as usize].flags.contains(DirFlags::IS_DIR) {
            let mut child_opt = arena[node_idx as usize].first_child;
            while let Some(child_idx) = child_opt {
                Self::set_selection_recursive(arena, child_idx, select);
                child_opt = arena[child_idx as usize].next_sibling;
            }
        }
    }

    // Also a static method now.
    fn update_parent_selection_state(arena: &mut Vec<DirNode>, parent_idx: Idx) {
        if parent_idx == 0 {
            return;
        }

        let mut child_opt = arena[parent_idx as usize].first_child;
        let mut all_children_selected = true;

        // If there are no children, the parent's state is its own. Don't change it based on this check.
        if child_opt.is_none() {
            return;
        }

        while let Some(child_idx) = child_opt {
            if !arena[child_idx as usize].flags.contains(DirFlags::SELECTED) {
                all_children_selected = false;
                break;
            }
            child_opt = arena[child_idx as usize].next_sibling;
        }

        let parent_node = &mut arena[parent_idx as usize];
        if all_children_selected {
            parent_node.flags.insert(DirFlags::SELECTED);
        } else {
            parent_node.flags.remove(DirFlags::SELECTED);
        }
    }

    // A helper to check for tri-state (`[~]`)
    pub fn has_partial_selection(&self, node_idx: Idx) -> bool {
        let node = &self.arena[node_idx as usize];
        node.flags.contains(DirFlags::PARTIAL_SELECTION)
    }

    /// Gets the full path of a node as a `PathBuf`.
    pub fn get_path_buf(arena: &[DirNode], node_idx: Idx) -> PathBuf {
        let mut path_parts = Vec::new();
        let mut current_idx = Some(node_idx);
        while let Some(idx) = current_idx {
            if idx == 0 {
                break;
            } // Stop at root
            path_parts.push(arena[idx as usize].name.clone());
            current_idx = arena[idx as usize].parent;
        }

        // Build the PathBuf from the reversed parts
        path_parts.iter().rev().collect()
    }

    /// Gets the full path of a node by walking up to the root.
    pub fn get_path(arena: &[DirNode], node_idx: Idx) -> String {
        let mut path_parts = Vec::new();
        let mut current_idx = Some(node_idx);
        while let Some(idx) = current_idx {
            if idx == 0 {
                break;
            } // Stop at root
            path_parts.push(arena[idx as usize].name.clone());
            current_idx = arena[idx as usize].parent;
        }
        path_parts.reverse();
        path_parts.join("/")
    }

    /// Returns a list of all selected paths.  For a selected directory we push
    /// its own path (so it works as a prefix filter) and DO NOT enumerate the
    /// thousands of children – that is expensive and unnecessary.
    pub fn get_selected_paths(&self) -> Vec<PathBuf> {
        let mut out = Vec::new();
        let mut already_added = std::collections::HashSet::new(); // store node idx

        for &idx in &self.visible_nodes {
            let node = &self.arena[idx as usize];
            if !node.flags.contains(DirFlags::SELECTED) {
                continue; // not selected
            }

            // If any ancestor is already in the set we can skip:
            let mut anc = node.parent;
            let mut covered = false;
            while let Some(p) = anc {
                if already_added.contains(&p) {
                    covered = true;
                    break;
                }
                anc = self.arena[p as usize].parent;
            }
            if covered {
                continue;
            }

            // Add this node’s path (dir or file)
            out.push(Self::get_path_buf(&self.arena, idx));
            already_added.insert(idx);
        }
        out
    }
}
