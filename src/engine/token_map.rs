use crate::engine::model::{EntryMetadata, ProcessedEntry, TokenMapEntry, TreeNode};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

#[derive(Debug, Clone, Eq, PartialEq)]
struct NodePriority {
    tokens: usize,
    path: String,
    depth: usize,
}

impl Ord for NodePriority {
    fn cmp(&self, other: &Self) -> Ordering {
        self.tokens
            .cmp(&other.tokens)
            .then_with(|| other.depth.cmp(&self.depth))
            .then_with(|| self.path.cmp(&other.path))
    }
}

impl PartialOrd for NodePriority {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

pub fn generate_token_map_with_limit(
    entries: &[ProcessedEntry],
    max_lines: Option<usize>,
    min_percent: Option<f64>,
) -> Vec<TokenMapEntry> {
    let max_lines = max_lines.unwrap_or(20);
    let min_percent = min_percent.unwrap_or(0.1);
    let mut root = TreeNode::with_path(String::new());

    for entry in entries.iter().filter(|e| e.is_file) {
        if let Some(tokens) = entry.token_count {
            // Only process entries that have tokens to avoid cluttering the map.
            if tokens == 0 {
                continue;
            }

            let path_str = entry.relative_path.to_string_lossy();
            // The insert_path function expects path components.
            let components: Vec<&str> = path_str.split('/').collect();

            // This metadata is for the file node itself.
            let metadata = EntryMetadata {
                is_dir: false,
                // ProcessedEntry doesn't track symlinks, so `false` is a safe default.
                is_symlink: false,
            };

            // Call the helper to recursively build the tree and aggregate token counts.
            insert_path(&mut root, &components, tokens, String::new(), metadata);
        }
    }

    let total_tokens = root.children.values().map(|child| child.tokens).sum();
    root.tokens = total_tokens;

    let allowed_nodes = select_nodes_to_display(&root, total_tokens, max_lines, min_percent);
    let mut entries = Vec::new();
    rebuild_filtered_tree(
        &root,
        String::new(),
        &allowed_nodes,
        &mut entries,
        0,
        total_tokens,
        true,
    );

    let displayed_tokens: usize = entries
        .iter()
        .map(|e| if !e.metadata.is_dir { e.tokens } else { 0 })
        .sum();

    let hidden_tokens = calculate_file_tokens(&root).saturating_sub(displayed_tokens);
    if hidden_tokens > 0 && total_tokens > 0 {
        entries.push(TokenMapEntry {
            path: "(other files)".to_string(),
            name: "(other files)".to_string(),
            tokens: hidden_tokens,
            percentage: (hidden_tokens as f64 / total_tokens as f64) * 100.0,
            depth: 0,
            is_last: true,
            metadata: EntryMetadata {
                is_dir: false,
                is_symlink: false,
            },
        });
    }

    entries
}

fn calculate_file_tokens(node: &TreeNode) -> usize {
    let mut current_node_tokens = 0;
    if node.metadata.is_some_and(|m| !m.is_dir) {
        current_node_tokens = node.tokens;
    }
    let children_tokens: usize = node.children.values().map(calculate_file_tokens).sum();
    current_node_tokens + children_tokens
}

fn insert_path(
    node: &mut TreeNode,
    components: &[&str],
    tokens: usize,
    parent_path: String,
    file_metadata: EntryMetadata,
) {
    if components.is_empty() {
        return;
    }
    let current_component_name = components[0].to_string();
    let current_full_path = if parent_path.is_empty() {
        current_component_name.clone()
    } else {
        format!("{parent_path}/{current_component_name}")
    };
    let child_node = node
        .children
        .entry(current_component_name)
        .or_insert_with(|| TreeNode::with_path(current_full_path.clone()));

    if components.len() == 1 {
        child_node.tokens = tokens;
        child_node.metadata = Some(file_metadata);
    } else {
        child_node.tokens += tokens;
        child_node.metadata = Some(EntryMetadata {
            is_dir: true,
            is_symlink: false,
        });
        insert_path(
            child_node,
            &components[1..],
            tokens,
            current_full_path,
            file_metadata,
        );
    }
}

fn select_nodes_to_display(
    root: &TreeNode,
    total_tokens: usize,
    max_lines: usize,
    min_percent: f64,
) -> HashMap<String, usize> {
    let mut heap = BinaryHeap::new();
    let mut allowed_nodes = HashMap::new();
    let min_tokens = if total_tokens == 0 {
        0
    } else {
        (total_tokens as f64 * min_percent / 100.0).ceil() as usize
    };

    for child in root.children.values() {
        if child.tokens >= min_tokens || child.metadata.is_some_and(|m| m.is_dir) {
            heap.push(NodePriority {
                tokens: child.tokens,
                path: child.path.clone(),
                depth: 0,
            });
        }
    }

    while allowed_nodes.len() < max_lines.saturating_sub(1) && !heap.is_empty() {
        if let Some(node_priority) = heap.pop() {
            if allowed_nodes.contains_key(&node_priority.path) {
                continue;
            }
            allowed_nodes.insert(node_priority.path.clone(), node_priority.depth);

            if let Some(node) = find_node_by_path(root, &node_priority.path) {
                for child in node.children.values() {
                    if (child.tokens >= min_tokens || child.metadata.is_some_and(|m| m.is_dir))
                        && !allowed_nodes.contains_key(&child.path)
                    {
                        heap.push(NodePriority {
                            tokens: child.tokens,
                            path: child.path.clone(),
                            depth: node_priority.depth + 1,
                        });
                    }
                }
            }
        }
    }
    allowed_nodes
}

fn find_node_by_path<'a>(root: &'a TreeNode, path_str: &str) -> Option<&'a TreeNode> {
    if path_str.is_empty() {
        return Some(root);
    }
    let components: Vec<&str> = path_str.split('/').collect();
    let mut current = root;
    for component in components {
        if component.is_empty() {
            continue;
        }
        match current.children.get(component) {
            Some(child) => current = child,
            None => return None,
        }
    }
    Some(current)
}

fn rebuild_filtered_tree(
    node: &TreeNode,
    current_path_str: String,
    allowed_nodes: &HashMap<String, usize>,
    entries: &mut Vec<TokenMapEntry>,
    depth: usize,
    total_tokens: usize,
    is_last_parent_child: bool,
) {
    if !current_path_str.is_empty() && allowed_nodes.contains_key(&current_path_str) {
        let percentage = if total_tokens > 0 {
            (node.tokens as f64 / total_tokens as f64) * 100.0
        } else {
            0.0
        };
        let name = current_path_str
            .split('/')
            .last()
            .unwrap_or(&current_path_str)
            .to_string();
        let metadata = node.metadata.unwrap_or(EntryMetadata {
            is_dir: !node.children.is_empty(),
            is_symlink: false,
        });
        entries.push(TokenMapEntry {
            path: current_path_str.clone(),
            name,
            tokens: node.tokens,
            percentage,
            depth,
            is_last: is_last_parent_child,
            metadata,
        });
    }
    let mut displayable_children: Vec<_> = node
        .children
        .iter()
        .filter(|(_, child_node)| allowed_nodes.contains_key(&child_node.path))
        .collect();
    displayable_children.sort_by(|a, b| b.1.tokens.cmp(&a.1.tokens));
    let num_displayable_children = displayable_children.len();
    for (i, (_child_name, child_node)) in displayable_children.iter().enumerate() {
        let is_last_child_to_display = i == num_displayable_children - 1;
        rebuild_filtered_tree(
            child_node,
            child_node.path.clone(),
            allowed_nodes,
            entries,
            depth + 1,
            total_tokens,
            is_last_child_to_display,
        );
    }
}
