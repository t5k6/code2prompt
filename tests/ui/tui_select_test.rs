#[cfg(feature = "tui")]
use code2prompt_tui::ui::tree_pane::TreePane;
#[cfg(feature = "tui")]
use code2prompt_tui::ui::tui_select::{App, AppMode, ListPane, Pane, handle_mouse_click};

#[cfg(feature = "tui")]
fn create_test_app() -> App {
    App {
        repo_name: "test".to_string(),
        extensions: ListPane::new(
            "Test Extensions".to_string(),
            vec![
                ("rs".to_string(), 100),
                ("toml".to_string(), 50),
                ("md".to_string(), 25),
                ("txt".to_string(), 10),
                ("json".to_string(), 75),
                ("yaml".to_string(), 30),
            ],
            None, // No initial selection
            |item| &item.0,
        ),
        directories: TreePane::new(vec![], None),
        active_pane: Pane::Extensions,
        mode: AppMode::Normal,
        list_render_buffer: Vec::new(),
        tree_render_buffer: Vec::new(),
        footer_buffer: Vec::new(),
    }
}

#[cfg(feature = "tui")]
#[test]
fn test_mouse_click_with_scroll() {
    let mut app = create_test_app();

    // Simulate the list being scrolled down by 2 items.
    // The item at index 4 ("json") would now appear on visual row 4 (2-indexed content).
    // Row 1: Title, Row 2: Border, Row 3: "rs", Row 4: "toml", etc.
    // If we click visual row 4, that's content row 2. With an offset of 2,
    // the target index is 2 + 2 = 4.
    *app.extensions.state.offset_mut() = 2;
    handle_mouse_click(&mut app, 4, 10); // Click on visual row 4 (1-based from top of screen)
    // The click handler should select index 2 in the filtered list.
    assert_eq!(app.extensions.state.selected(), Some(2));
}
