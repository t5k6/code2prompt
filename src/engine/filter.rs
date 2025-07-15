//! This module contains the logic for filtering files based on include and exclude patterns.
#[cfg(feature = "colors")]
use colored::*;
use globset::GlobSet;
use log::debug;
use std::path::Path;

/// Determines whether a file should be included based on include and exclude patterns.
///
/// # Arguments
///
/// * `path` - The path to the file to be checked.
/// * `include_patterns` - A slice of strings representing the include patterns.
/// * `exclude_patterns` - A slice of strings representing the exclude patterns.
/// * `include_priority` - A boolean indicating whether to give priority to include patterns if both include and exclude patterns match.
///
/// # Returns
///
/// * `bool` - `true` if the file should be included, `false` otherwise.
pub fn should_include_file(
    path: &Path,
    root_path: &Path,
    include_set: &GlobSet,
    exclude_set: &GlobSet,
    include_priority: bool,
) -> bool {
    // Performance: If no patterns are specified, all files are included by default.
    // This avoids a `canonicalize` syscall for every path.
    if include_set.is_empty() && exclude_set.is_empty() {
        return true;
    }

    let relative_path = path.strip_prefix(root_path).unwrap_or(path);
    let path_str = relative_path.to_string_lossy().replace('\\', "/");

    // CHANGE to use is_match
    let included = include_set.is_match(&path_str);
    let excluded = exclude_set.is_match(&path_str);

    // ~~~ Decision ~~~
    let result = match (included, excluded) {
        (true, true) => include_priority,
        (true, false) => true,
        (false, true) => false,
        // CHANGE: if no include patterns are provided, include everything NOT excluded.
        (false, false) => include_set.is_empty(),
    };

    #[cfg(feature = "colors")]
    debug!(
        "Checking path: {:?}, {}: {}, {}: {}, decision: {}",
        path_str,
        "included".bold().green(),
        included,
        "excluded".bold().red(),
        excluded,
        result
    );
    #[cfg(not(feature = "colors"))]
    debug!(
        "Checking path: {:?}, included: {}, excluded: {}, decision: {}",
        path_str, included, excluded, result
    );
    result
}
