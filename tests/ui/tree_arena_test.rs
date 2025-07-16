use code2prompt_tui::ui::tree_arena::{IS_DIR, PathInfo, build_dir_arena};

// Test struct for the PathInfo trait
struct TestPath {
    path_str: String,
    count_val: usize,
    ext_val: Option<String>,
}

impl PathInfo for TestPath {
    fn path(&self) -> &str {
        &self.path_str
    }
    fn count(&self) -> usize {
        self.count_val
    }
    fn extension(&self) -> Option<&String> {
        self.ext_val.as_ref()
    }
}

#[test]
fn build_arena_basic() {
    let paths = vec![
        TestPath {
            path_str: "src/ui/tui.rs".to_string(),
            count_val: 1,
            ext_val: Some("rs".to_string()),
        },
        TestPath {
            path_str: "src/ui/cli.rs".to_string(),
            count_val: 1,
            ext_val: Some("rs".to_string()),
        },
        TestPath {
            path_str: "src/lib.rs".to_string(),
            count_val: 1,
            ext_val: Some("rs".to_string()),
        },
        TestPath {
            path_str: "README.md".to_string(),
            count_val: 1,
            ext_val: Some("md".to_string()),
        },
    ];
    let arena = build_dir_arena(&paths);

    // We expect: root, src, ui, tui.rs, cli.rs, lib.rs, README.md => 7 nodes
    assert_eq!(arena.len(), 7, "Expected 7 nodes in the arena");

    // Find the root node (always at index 0)
    let root = &arena[0];
    assert!(root.flags & IS_DIR != 0);
    assert_eq!(root.count, 4, "Root count should be total files");

    // Find 'src' and check its count
    let src_idx = arena
        .iter()
        .position(|n| n.name == "src")
        .expect("'src' node not found") as u32;
    assert_eq!(arena[src_idx as usize].count, 3, "'src' count should be 3");
    assert!(arena[src_idx as usize].flags & IS_DIR != 0);

    // Find 'ui' and check its count
    let ui_idx = arena
        .iter()
        .position(|n| n.name == "ui")
        .expect("'ui' node not found") as u32;
    assert_eq!(arena[ui_idx as usize].count, 2, "'ui' count should be 2");
    assert!(arena[ui_idx as usize].flags & IS_DIR != 0);

    // Find a file and check its flags and count
    let readme_idx = arena
        .iter()
        .position(|n| n.name == "README.md")
        .expect("'README.md' node not found") as u32;
    assert_eq!(
        arena[readme_idx as usize].count, 1,
        "'README.md' count should be 1"
    );
    assert!(
        arena[readme_idx as usize].flags & IS_DIR == 0,
        "README.md should not be a directory"
    );
}
