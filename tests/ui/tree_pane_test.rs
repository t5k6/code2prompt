use code2prompt_tui::ui::tree_arena::{PathInfo, SELECTED, build_dir_arena};
use code2prompt_tui::ui::tree_pane::TreePane;
use ratatui::widgets::ListState;
use std::collections::HashSet;

struct TestPath(String);
impl PathInfo for TestPath {
    fn path(&self) -> &str {
        &self.0
    }
    fn count(&self) -> usize {
        1
    }
    fn extension(&self) -> Option<&String> {
        None
    }
}

#[test]
fn test_get_selected_paths_with_nesting() {
    let paths = vec![
        TestPath("src/main.rs".to_string()),
        TestPath("src/lib.rs".to_string()),
        TestPath("src/ui/tui.rs".to_string()),
        TestPath("README.md".to_string()),
    ];

    let mut arena = build_dir_arena(&paths);

    // Manually find and select the 'src' directory node
    let src_node_idx = arena
        .iter()
        .position(|n| n.name == "src")
        .expect("Could not find 'src' node") as u32;

    // Deselect everything first for a clean state
    for node in &mut arena {
        node.flags &= !SELECTED;
    }

    // Select the 'src' node
    arena[src_node_idx as usize].flags |= SELECTED;

    let mut pane = TreePane {
        arena,
        visible_nodes: vec![1, 2, 3, 4, 5, 6], // A dummy list for the test
        cursor: 0,
        list_state: ListState::default(),
        last_filter: HashSet::new(),
    };

    let selected_paths = pane.get_selected_paths();

    // Since the parent 'src' is selected, only it should be returned.
    // The children ('src/main.rs', 'src/lib.rs') should not be included
    // because their path is covered by the parent.
    assert_eq!(
        selected_paths,
        vec!["src".to_string()],
        "Should only return the top-most selected path 'src'"
    );
}
