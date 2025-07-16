use code2prompt_tui::engine::filter::should_include_file;
use globset::{Glob, GlobSet, GlobSetBuilder};
use quickcheck::TestResult;
use std::path::{Path, PathBuf};

/// Helper to build a GlobSet from a slice of string patterns.
/// Invalid patterns are ignored.
fn build_globset(patterns: &[String]) -> GlobSet {
    let mut builder = GlobSetBuilder::new();
    for p in patterns {
        if let Ok(glob) = Glob::new(p) {
            builder.add(glob);
        }
    }
    builder.build().unwrap()
}

#[test]
fn test_only_include_filters_correctly() {
    let include_set = build_globset(&["**/*.rs".to_string()]);
    let exclude_set = build_globset(&[]);
    let root = PathBuf::from(".");

    // Should be included
    assert!(should_include_file(
        &PathBuf::from("src/main.rs"),
        &root,
        &include_set,
        &exclude_set,
        false
    ));
    // Should be excluded because it doesn't match the include pattern
    assert!(!should_include_file(
        &PathBuf::from("README.md"),
        &root,
        &include_set,
        &exclude_set,
        false
    ));
}

#[test]
fn test_only_exclude_filters_correctly() {
    let include_set = build_globset(&[]);
    let exclude_set = build_globset(&["**/*.log".to_string()]);
    let root = PathBuf::from(".");

    // Should be included because it doesn't match the exclude pattern
    assert!(should_include_file(
        &PathBuf::from("src/main.rs"),
        &root,
        &include_set,
        &exclude_set,
        false
    ));
    // Should be excluded
    assert!(!should_include_file(
        &PathBuf::from("debug.log"),
        &root,
        &include_set,
        &exclude_set,
        false
    ));
}

#[quickcheck::quickcheck]
fn prop_no_patterns_includes_all(path: String) -> TestResult {
    if path.contains('\\') {
        return TestResult::discard();
    }
    let include_set = build_globset(&[]);
    let exclude_set = build_globset(&[]);

    TestResult::from_bool(should_include_file(
        &PathBuf::from(&path),
        &PathBuf::from("."),
        &include_set,
        &exclude_set,
        false,
    ))
}

#[quickcheck::quickcheck]
fn prop_exclusion_overrides_implicit_inclusion(path: String, pattern: String) -> TestResult {
    if Glob::new(&pattern).is_err() || path.contains('\\') {
        return TestResult::discard();
    }
    let include_set = build_globset(&[]);
    let exclude_set = build_globset(&[pattern.clone()]); // Use clone here

    let is_match = exclude_set.is_match(&path);
    let should_be_included = should_include_file(
        &PathBuf::from(&path),
        &PathBuf::from("."),
        &include_set,
        &exclude_set,
        false,
    );

    TestResult::from_bool(is_match != should_be_included)
}

#[quickcheck::quickcheck]
fn prop_include_priority_works_on_conflict(path: String, pattern: String) -> TestResult {
    if Glob::new(&pattern).is_err() || path.contains('\\') {
        return TestResult::discard();
    }
    let glob_set = build_globset(&[pattern.clone()]); // Use clone here

    if !glob_set.is_match(&path) {
        return TestResult::discard();
    }

    let with_priority = should_include_file(
        &PathBuf::from(&path),
        &PathBuf::from("."),
        &glob_set,
        &glob_set,
        true,
    );

    let without_priority = should_include_file(
        &PathBuf::from(&path),
        &PathBuf::from("."),
        &glob_set,
        &glob_set,
        false,
    );

    TestResult::from_bool(with_priority && !without_priority)
}
