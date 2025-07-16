#[cfg(not(any(feature = "cache", feature = "tui")))]
pub use std::collections::HashMap;

#[cfg(any(feature = "cache", feature = "tui"))]
pub use rustc_hash::FxHashMap as HashMap;

pub fn merge_usize(into: &mut HashMap<String, usize>, from: HashMap<String, usize>) {
    for (k, v) in from {
        *into.entry(k).or_default() += v;
    }
}
