//! This module contains the logic for filtering files based on include and exclude patterns.
use std::path::Path;

use colored::Colorize;
use globset::GlobSet;
#[cfg(feature = "logging")]
use log::debug;

// An explicit struct to make the matching logic clear and testable.
#[derive(Debug, Default)]
struct MatchResult {
    included: bool,
    excluded: bool,
}

// A helper function to contain the matching logic.
fn get_match_result(path_str: &str, include_set: &GlobSet, exclude_set: &GlobSet) -> MatchResult {
    // If no include patterns are given, we assume inclusion unless excluded.
    // If include patterns *are* given, we require a match.
    let included = include_set.is_empty() || include_set.is_match(path_str);

    // We only need to check for exclusion if the file is considered for inclusion.
    let excluded = if included {
        exclude_set.is_match(path_str)
    } else {
        false // Not included anyway, so can't be excluded.
    };

    MatchResult { included, excluded }
}

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
    if include_set.is_empty() && exclude_set.is_empty() {
        return true;
    }

    let relative_path = path.strip_prefix(root_path).unwrap_or(path);
    let path_str = relative_path.to_string_lossy().replace('\\', "/");

    let matches = get_match_result(&path_str, include_set, exclude_set);

    // ~~~ Decision ~~~
    let result = match (matches.included, matches.excluded) {
        // Explicitly excluded.
        (_, true) => {
            // If it was also explicitly included, priority decides.
            if matches.included {
                include_priority
            } else {
                false
            }
        }
        // Included and not excluded.
        (true, false) => true,
        // Not included and not excluded (e.g., failed to match an include pattern).
        (false, false) => false,
    };

    #[cfg(all(feature = "logging", feature = "colors"))]
    debug!(
        "Checking path: {:?}, {}: {}, {}: {}, decision: {}",
        path_str,
        "included".bold().green(),
        matches.included,
        "excluded".bold().red(),
        matches.excluded,
        result
    );
    #[cfg(all(feature = "logging", not(feature = "colors")))]
    debug!(
        "Checking path: {:?}, included: {}, excluded: {}, decision: {}",
        path_str, matches.included, matches.excluded, result
    );
    result
}
