use std::{cell::RefCell, fs, path::Path, sync::Arc, time::SystemTime};

use anyhow::{Context, Result};
use crossbeam_channel::{Sender, unbounded};
use globset::GlobSet;
use ignore::{DirEntry, WalkBuilder, WalkState};
#[cfg(feature = "logging")]
use log::warn;
use sha2::{Digest, Sha256};

use crate::common::{
    code,
    glob::build_globset,
    hash::{HashMap, merge_usize},
    path::{self},
};
use crate::engine::{
    cache::ScanCache, config::Code2PromptConfig, filter::should_include_file,
    model::ProcessedEntry, token::count_tokens,
};

const MAX_FILE_SIZE_BYTES: u64 = 1_048_576; // 1 MiB

// ────────────────────────────────────────────────────────────
// Public enum (unchanged)
// ────────────────────────────────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessingMode {
    FullProcess,
    ExtensionCollection,
}

// ────────────────────────────────────────────────────────────
// Private payloads sent once per worker thread
// ────────────────────────────────────────────────────────────
enum Batch {
    Entries(Vec<ProcessedEntry>),
    Ext(HashMap<String, usize>),
    Dir(HashMap<String, usize>),
}

// ────────────────────────────────────────────────────────────
// One Worker per thread – aggregates locally, emits in Drop
// ────────────────────────────────────────────────────────────
struct Worker {
    mode: ProcessingMode,
    cfg: Arc<Code2PromptConfig>,
    tx: Sender<Batch>,

    // only allocated when needed
    entries: Vec<ProcessedEntry>,
    ext_cnt: HashMap<String, usize>,
    dir_cnt: HashMap<String, usize>,
}

impl Worker {
    fn new(mode: ProcessingMode, cfg: Arc<Code2PromptConfig>, tx: Sender<Batch>) -> Self {
        Self {
            mode,
            cfg,
            tx,
            entries: Vec::new(),
            ext_cnt: HashMap::default(),
            dir_cnt: HashMap::default(),
        }
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        match self.mode {
            ProcessingMode::FullProcess if !self.entries.is_empty() => {
                let _ = self
                    .tx
                    .send(Batch::Entries(std::mem::take(&mut self.entries)));
            }
            ProcessingMode::ExtensionCollection => {
                if !self.ext_cnt.is_empty() {
                    let _ = self.tx.send(Batch::Ext(std::mem::take(&mut self.ext_cnt)));
                }
                if !self.dir_cnt.is_empty() {
                    let _ = self.tx.send(Batch::Dir(std::mem::take(&mut self.dir_cnt)));
                }
            }
            _ => {}
        }
    }
}

// ────────────────────────────────────────────────────────────
// Thread-local cache handle
// ────────────────────────────────────────────────────────────
thread_local! {
    static THREAD_CACHE: RefCell<Option<ScanCache>> = RefCell::new(None);
}

// ────────────────────────────────────────────────────────────
// Public entry point
// ────────────────────────────────────────────────────────────
pub fn process_codebase(
    cfg: &Code2PromptConfig,
    mode: ProcessingMode,
) -> Result<(
    Vec<ProcessedEntry>,
    HashMap<String, usize>,
    HashMap<String, usize>,
)> {
    let include_glob = build_globset(&cfg.include_patterns)?;
    let exclude_glob = build_globset(&cfg.exclude_patterns)?;

    let root = cfg
        .path
        .canonicalize()
        .with_context(|| format!("Failed to canonicalize {}", cfg.path.display()))?;

    // Single channel for all workers
    let (tx, rx) = unbounded::<Batch>();

    // ── start parallel walker ───────────────────────────────
    WalkBuilder::new(&root)
        .follow_links(cfg.follow_symlinks)
        .hidden(!cfg.hidden)
        .git_ignore(!cfg.no_ignore)
        .build_parallel()
        .run(|| {
            let tx = tx.clone();
            let cfg = Arc::new(cfg.clone());
            let inc = include_glob.clone();
            let exc = exclude_glob.clone();
            let root = root.clone();

            let mut w = Worker::new(mode, cfg, tx);

            Box::new(move |res| {
                THREAD_CACHE.with(|c| {
                    // Lazily initialize the cache for this thread if needed.
                    if w.cfg.cache && c.borrow().is_none() {
                        *c.borrow_mut() = ScanCache::open(&root).ok();
                    }

                    // Now, handle the entry using the cache reference from within the closure.
                    // c.borrow().as_ref() correctly yields an `Option<&ScanCache>`.
                    handle_entry(res, &root, &inc, &exc, &mut w, c.borrow().as_ref());
                });

                WalkState::Continue
            })
        });

    drop(tx); // close channel

    // ── Aggregate batches ───────────────────────────────────
    let mut entries = Vec::new();
    let mut ext_cnt = HashMap::default();
    let mut dir_cnt = HashMap::default();

    while let Ok(batch) = rx.recv() {
        match batch {
            Batch::Entries(mut v) => entries.append(&mut v),
            Batch::Ext(m) => merge_usize(&mut ext_cnt, m),
            Batch::Dir(m) => merge_usize(&mut dir_cnt, m),
        }
    }

    Ok((entries, ext_cnt, dir_cnt))
}

// ────────────────────────────────────────────────────────────
//  Per-entry processing (runs inside worker closure)
// ────────────────────────────────────────────────────────────
fn handle_entry(
    res: Result<DirEntry, ignore::Error>,
    root: &Path,
    inc: &GlobSet,
    exc: &GlobSet,
    w: &mut Worker,
    cache: Option<&ScanCache>,
) {
    let entry = match res {
        Ok(e) => e,
        Err(e) => {
            #[cfg(feature = "logging")]
            warn!("Walk error: {e}");
            return;
        }
    };

    if !should_include_file(entry.path(), root, inc, exc, w.cfg.include_priority) {
        return;
    }
    if !entry.file_type().is_some_and(|ft| ft.is_file()) {
        return; // skip dirs/symlinks here
    }

    match w.mode {
        ProcessingMode::ExtensionCollection => collect_ext_dir(entry.path(), root, w),
        ProcessingMode::FullProcess => process_file(entry.path(), root, w, cache),
    }
}

// ────────────────────────────────────────────────────────────
//  ExtensionCollection fast path
// ────────────────────────────────────────────────────────────
fn collect_ext_dir(path: &Path, root: &Path, w: &mut Worker) {
    // directory counter
    if let Some(parent) = path.parent().and_then(|p| p.strip_prefix(root).ok()) {
        if !parent.as_os_str().is_empty() {
            let key = path::to_fwd_slash(parent);
            *w.dir_cnt.entry(key).or_default() += 1;
        }
    }
    // extension counter
    if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
        *w.ext_cnt.entry(ext.to_ascii_lowercase()).or_default() += 1;
    }
}

// ────────────────────────────────────────────────────────────
//  FullProcess path
// ────────────────────────────────────────────────────────────
fn process_file(path: &Path, root: &Path, w: &mut Worker, cache: Option<&ScanCache>) {
    // --- Calculate relative path ONCE at the top ---
    let rel_path = path.strip_prefix(root).unwrap_or(path);
    let rel_path_str = path::to_fwd_slash(rel_path);

    // ------- cache fast path -------
    if let Ok(md) = fs::metadata(path) {
        if md.len() == 0 || md.len() > MAX_FILE_SIZE_BYTES {
            return;
        }
        let mtime = md.modified().ok();
        // The `rel_path_str` is already calculated above
        if let (Some(c), Some(mt)) = (cache, mtime) {
            if let Ok(Some(hit)) = c.lookup(&rel_path_str, mt, md.len()) {
                // CACHE HIT: Create entry with `code: None`. No I/O!
                w.entries.push(make_entry(
                    path,
                    rel_path,
                    None, // Pass None for code
                    &w.cfg,
                    Some(hit.token_count),
                    Some(mt),
                ));
                return;
            }
        }
    }

    // ------- slow path -------
    let code = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            #[cfg(feature = "logging")]
            warn!("Skipping {} ({e})", path.display());
            return;
        }
    };

    // --- (passing rel_path) ---
    let mut entry = make_entry(
        path,
        rel_path, // pass the pre-calculated relative path
        Some(&code),
        &w.cfg,
        None,
        None,
    );

    if w.cfg.token_map_enabled {
        entry.token_count = count_tokens(&code, w.cfg.tokenizer).ok();
    }

    // insert into cache
    if let (Some(c), Some(tok)) = (cache, entry.token_count) {
        if let Ok(md) = fs::metadata(path) {
            if let Ok(mt) = md.modified() {
                let digest = Sha256::digest(code.as_bytes());
                // Use the `rel_path_str` from the top of the function
                let _ = c.insert(&rel_path_str, mt, md.len(), digest.into(), tok, Some(&code));
            }
        }
    }

    w.entries.push(entry);
}

// ────────────────────────────────────────────────────────────
//  Utils
// ────────────────────────────────────────────────────────────
fn make_entry(
    path: &Path,
    relative_path: &Path,
    code_str: Option<&str>,
    cfg: &Code2PromptConfig,
    tok_cnt: Option<usize>,
    mtime: Option<SystemTime>,
) -> ProcessedEntry {
    let ext = path.extension().and_then(|e| e.to_str()).map(str::to_owned);
    let wrapped_code = code_str.map(|c| {
        code::wrap(
            c,
            ext.as_deref().unwrap_or(""),
            cfg.line_numbers,
            cfg.no_codeblock,
        )
    });
    ProcessedEntry {
        path: path.to_path_buf(),
        relative_path: relative_path.to_path_buf(),
        is_file: true,
        code: wrapped_code,
        extension: ext,
        token_count: tok_cnt,
        mtime,
    }
}
