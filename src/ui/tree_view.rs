use std::path::Path;

use crate::common::format::format_path_label;
use crate::engine::model::ProcessedEntry;

/// Builds a string representation of the directory tree for display.
pub fn build_tree_view(
    root_path: &Path,
    entries: &[ProcessedEntry],
    full_directory_tree: bool,
) -> String {
    use termtree::Tree;

    let canonical_root = root_path
        .canonicalize()
        .unwrap_or_else(|_| root_path.to_path_buf());
    let mut root_tree = Tree::new(format_path_label(&canonical_root));

    if !full_directory_tree {
        let mut leaves: Vec<_> = entries
            .iter()
            .map(|e| Tree::new(e.relative_path.to_string_lossy().into_owned()))
            .collect();
        leaves.sort_by(|a, b| a.root.cmp(&b.root));
        root_tree.leaves = leaves;
    } else {
        let mut sorted_entries = entries.to_vec();
        sorted_entries.sort_by(|a, b| a.path.cmp(&b.path));
        for e in &sorted_entries {
            if let Ok(rel) = e.path.strip_prefix(&canonical_root) {
                let mut cur = &mut root_tree;
                for comp in rel.components() {
                    let s = comp.as_os_str().to_string_lossy().into_owned();
                    cur = if let Some(pos) = cur.leaves.iter_mut().position(|t| t.root == s) {
                        &mut cur.leaves[pos]
                    } else {
                        cur.leaves.push(Tree::new(s));
                        cur.leaves.last_mut().unwrap()
                    };
                }
            }
        }
    }
    root_tree.to_string()
}
