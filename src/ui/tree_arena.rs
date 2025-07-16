use bitflags::bitflags;

use crate::common::hash::HashMap;

// ──────────────────────────────────────────────────────────────
//  Public data structures
// ──────────────────────────────────────────────────────────────

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
    pub struct DirFlags: u8 {
        const IS_DIR    = 0b0000_0001;
        const EXPANDED  = 0b0000_0010;
        const SELECTED  = 0b0000_0100;
        const PARTIAL_SELECTION  = 0b0000_1000;
    }
}

pub type Idx = u32; // 4 bytes – supports huge repos

pub trait PathInfo {
    fn path(&self) -> &str;
    fn count(&self) -> usize;
    fn extension(&self) -> Option<&String>;
    fn token_count(&self) -> Option<usize>;
}

#[derive(Debug)]
pub struct DirNode {
    pub name: String,
    pub parent: Option<Idx>,
    pub first_child: Option<Idx>,
    pub next_sibling: Option<Idx>,
    pub flags: DirFlags,
    pub file_count: usize,
    pub total_toks: usize,
    pub visible_toks: usize,
    pub visible_files: usize,
    pub extension: Option<String>,
    pub ext_slot: u16,
}

// ──────────────────────────────────────────────────────────────
//  Arena builder
// ──────────────────────────────────────────────────────────────
/// Build an arena of `DirNode`s from a slice of paths (relative, e.g. `"src/ui/tui.rs"`).
/// `file_count` is typically 1, but letting the caller pass it in lets you
/// reuse the builder for “folder only” statistics as well.
///
/// Complexity:  O(total_components)  and   O(total_nodes) memory.
pub fn build_dir_arena<T: PathInfo>(
    paths: &[T],
    ext_to_slot: &HashMap<String, u16>,
) -> Vec<DirNode> {
    let mut index: HashMap<(Idx, String), Idx> = HashMap::default();

    // Arena; 0 == synthetic root
    let mut arena: Vec<DirNode> = Vec::with_capacity(paths.len() * 2);
    arena.push(DirNode {
        name: String::from("(root)"),
        parent: None,
        first_child: None,
        next_sibling: None,
        flags: DirFlags::IS_DIR | DirFlags::EXPANDED | DirFlags::SELECTED,
        file_count: 0,
        total_toks: 0,
        visible_toks: 0,
        visible_files: 0,
        extension: None,
        ext_slot: 0,
    });

    // ───── Main loop ───────────────────────────────────────────
    for path_info in paths {
        let path = path_info.path();
        if path.is_empty() {
            continue;
        }

        let mut parent = 0; // start at root
        let mut comps = std::path::Path::new(path).components().peekable();

        while let Some(comp) = comps.next() {
            let comp_str = comp.as_os_str().to_string_lossy();
            let is_last = comps.peek().is_none();
            let file_extension = if is_last { path_info.extension() } else { None };
            let ext_slot = file_extension
                .and_then(|ext| ext_to_slot.get(ext).copied())
                .unwrap_or(0); // Use 0 for "no extension" or unmapped
            let child = ensure_child(
                &mut arena,
                &mut index,
                parent,
                &comp_str,
                !is_last,
                file_extension,
                ext_slot,
            );

            // propagate file count up the chain (including file node itself)
            let file_count = path_info.count();
            let token_count = if is_last {
                path_info.token_count().unwrap_or(0)
            } else {
                0
            };

            if file_count > 0 {
                let mut node_idx_to_update = Some(child);
                while let Some(idx) = node_idx_to_update {
                    arena[idx as usize].file_count += file_count;
                    arena[idx as usize].total_toks += token_count;
                    arena[idx as usize].visible_toks += token_count;
                    arena[idx as usize].visible_files += file_count;
                    node_idx_to_update = arena[idx as usize].parent;
                }
            }

            parent = child;
        }
    }

    arena
}

// It takes mutable references to the arena and index, so its borrows are temporary.
fn ensure_child<'a>(
    arena: &'a mut Vec<DirNode>,
    index: &'a mut HashMap<(Idx, String), Idx>,
    parent_idx: Idx,
    part: &str,
    is_dir: bool,
    extension: Option<&String>,
    ext_slot_val: u16,
) -> Idx {
    let key = (parent_idx, part.to_string());
    if let Some(&idx) = index.get(&key) {
        return idx;
    }

    let new_idx: Idx = arena
        .len()
        .try_into()
        .expect("Too many nodes for u32 index");

    let new_flags = if is_dir {
        DirFlags::IS_DIR | DirFlags::SELECTED
    } else {
        DirFlags::SELECTED
    };

    let new_node = DirNode {
        name: part.to_string(),
        parent: Some(parent_idx),
        first_child: None,
        next_sibling: arena[parent_idx as usize].first_child,
        flags: new_flags,
        file_count: 0,
        total_toks: 0,
        visible_toks: 0,
        visible_files: 0,
        extension: extension.cloned(),
        ext_slot: ext_slot_val,
    };
    arena.push(new_node);
    arena[parent_idx as usize].first_child = Some(new_idx);
    index.insert(key, new_idx);
    new_idx
}
