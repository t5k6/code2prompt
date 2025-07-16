use code2prompt_tui::engine::filter::should_include_file;
use colored::*;
use globset::{Glob, GlobSet, GlobSetBuilder};
use once_cell::sync::Lazy;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use tempfile::{TempDir, tempdir};

fn create_temp_file(dir: &Path, name: &str, content: &str) {
    let file_path = dir.join(name);
    let parent_dir = file_path.parent().unwrap();
    fs::create_dir_all(parent_dir).expect(&format!("Failed to create directory: {:?}", parent_dir));
    let mut file =
        File::create(&file_path).expect(&format!("Failed to create temp file: {:?}", file_path));
    writeln!(file, "{}", content).expect(&format!("Failed to write to temp file: {:?}", file_path));
}

static TEST_DIR: Lazy<TempDir> = Lazy::new(|| {
    let dir = tempdir().expect("Failed to create a temp directory");
    create_test_hierarchy(dir.path());
    dir
});

fn create_test_hierarchy(base_path: &Path) {
    let lowercase_dir = base_path.join("lowercase");
    let uppercase_dir = base_path.join("uppercase");

    fs::create_dir_all(&lowercase_dir).expect("Failed to create lowercase directory");
    fs::create_dir_all(&uppercase_dir).expect("Failed to create uppercase directory");

    let files = vec![
        ("lowercase/foo.py", "content foo.py"),
        ("lowercase/bar.py", "content bar.py"),
        ("lowercase/baz.py", "content baz.py"),
        ("lowercase/qux.txt", "content qux.txt"),
        ("lowercase/corge.txt", "content corge.txt"),
        ("lowercase/grault.txt", "content grault.txt"),
        ("uppercase/FOO.py", "CONTENT FOO.PY"),
        ("uppercase/BAR.py", "CONTENT BAR.PY"),
        ("uppercase/BAZ.py", "CONTENT BAZ.PY"),
        ("uppercase/QUX.txt", "CONTENT QUX.TXT"),
        ("uppercase/CORGE.txt", "CONTENT CORGE.TXT"),
        ("uppercase/GRAULT.txt", "CONTENT GRAULT.TXT"),
    ];

    for (file_path, content) in files {
        create_temp_file(base_path, file_path, content);
    }
    println!(
        "{}{}{} {}",
        "[".bold().white(),
        "✓".bold().green(),
        "]".bold().white(),
        "Tempfiles created".green()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compile_patterns(patterns: &[&str]) -> GlobSet {
        let mut builder = GlobSetBuilder::new();
        for pattern in patterns {
            builder.add(Glob::new(pattern).unwrap());
        }
        builder.build().unwrap()
    }

    #[test]
    fn test_no_list() {
        let base_path = TEST_DIR.path();

        let include_patterns = compile_patterns(&[]);
        let exclude_patterns = compile_patterns(&[]);
        let include_priority = true;

        for file in [
            "lowercase/foo.py",
            "lowercase/bar.py",
            "lowercase/baz.py",
            "uppercase/FOO.py",
            "uppercase/BAR.py",
            "uppercase/BAZ.py",
            "lowercase/qux.txt",
            "lowercase/corge.txt",
            "lowercase/grault.txt",
            "uppercase/QUX.txt",
            "uppercase/CORGE.txt",
            "uppercase/GRAULT.txt",
        ] {
            let path = base_path.join(file);
            assert!(should_include_file(
                &path,
                base_path,
                &include_patterns,
                &exclude_patterns,
                include_priority
            ));
        }
    }

    #[test]
    fn test_include_patterns() {
        let base_path = TEST_DIR.path();

        // Test with a simple extension pattern
        let include_patterns_ext = compile_patterns(&["*.py"]);
        let exclude_patterns = compile_patterns(&[]);
        let include_priority = false;

        let path_ext = base_path.join("lowercase/foo.py");
        // The root path should be the base directory for glob matching
        assert!(should_include_file(
            &path_ext,
            base_path,
            &include_patterns_ext,
            &exclude_patterns,
            include_priority
        ));

        // Test with a globstar pattern
        let include_patterns_glob = compile_patterns(&["**/*.py"]);
        let path_glob = base_path.join("uppercase/FOO.py");
        assert!(should_include_file(
            &path_glob,
            base_path,
            &include_patterns_glob,
            &exclude_patterns,
            include_priority
        ));

        // Test a non-matching file
        let path_non_match = base_path.join("lowercase/qux.txt");
        assert!(!should_include_file(
            &path_non_match,
            base_path,
            &include_patterns_glob,
            &exclude_patterns,
            include_priority
        ));
    }

    #[test]
    fn test_exclude_patterns() {
        let base_path = TEST_DIR.path();

        let include_patterns = compile_patterns(&[]);
        let exclude_patterns = compile_patterns(&["**/*.txt"]);
        let include_priority = false;

        let path_to_check = base_path.join("lowercase/qux.txt");
        assert!(!should_include_file(
            &path_to_check,
            base_path,
            &include_patterns,
            &exclude_patterns,
            include_priority
        ));

        let path_to_include = base_path.join("lowercase/foo.py");
        assert!(should_include_file(
            &path_to_include,
            base_path,
            &include_patterns,
            &exclude_patterns,
            include_priority
        ));
    }

    #[test]
    fn test_include_files() {
        let base_path = TEST_DIR.path();

        let include_patterns = compile_patterns(&["**/foo.py", "**/bar.py"]);
        let exclude_patterns = compile_patterns(&[]);
        let include_priority = false;

        for file in ["lowercase/foo.py", "lowercase/bar.py"] {
            let path = base_path.join(file);
            assert!(should_include_file(
                &path,
                base_path,
                &include_patterns,
                &exclude_patterns,
                include_priority
            ));
        }

        for file in [
            "lowercase/baz.py",
            "lowercase/qux.txt",
            "lowercase/corge.txt",
            "lowercase/grault.txt",
            "uppercase/FOO.py",
            "uppercase/BAR.py",
            "uppercase/BAZ.py",
            "uppercase/QUX.txt",
            "uppercase/CORGE.txt",
            "uppercase/GRAULT.txt",
        ] {
            let path = base_path.join(file);
            assert!(!should_include_file(
                &path,
                base_path,
                &include_patterns,
                &exclude_patterns,
                include_priority
            ));
        }
    }

    #[test]
    fn test_exclude_files() {
        let base_path = TEST_DIR.path();

        let include_patterns = compile_patterns(&[]);
        let exclude_patterns = compile_patterns(&["**/foo.py", "**/bar.py"]);
        let include_priority = false;

        for file in ["lowercase/foo.py", "lowercase/bar.py"] {
            let path = base_path.join(file);
            assert!(!should_include_file(
                &path,
                base_path,
                &include_patterns,
                &exclude_patterns,
                include_priority
            ));
        }

        for file in [
            "lowercase/baz.py",
            "lowercase/qux.txt",
            "lowercase/corge.txt",
            "lowercase/grault.txt",
            "uppercase/FOO.py",
            "uppercase/BAR.py",
            "uppercase/BAZ.py",
            "uppercase/QUX.txt",
            "uppercase/CORGE.txt",
            "uppercase/GRAULT.txt",
        ] {
            let path = base_path.join(file);
            assert!(should_include_file(
                &path,
                base_path,
                &include_patterns,
                &exclude_patterns,
                include_priority
            ));
        }
    }

    #[test]
    fn test_include_exclude_conflict_file() {
        let base_path = TEST_DIR.path();

        let include_patterns = compile_patterns(&["**/foo.py"]);
        let exclude_patterns = compile_patterns(&["**/foo.py"]);
        let include_priority = true;

        for file in ["lowercase/foo.py"] {
            let path = base_path.join(file);
            assert!(should_include_file(
                &path,
                base_path,
                &include_patterns,
                &exclude_patterns,
                include_priority
            ));
        }

        for file in [
            "lowercase/bar.py",
            "lowercase/baz.py",
            "lowercase/qux.txt",
            "lowercase/corge.txt",
            "lowercase/grault.txt",
            "uppercase/FOO.py",
            "uppercase/BAR.py",
            "uppercase/BAZ.py",
            "uppercase/QUX.txt",
            "uppercase/CORGE.txt",
            "uppercase/GRAULT.txt",
        ] {
            let path = base_path.join(file);
            assert!(!should_include_file(
                &path,
                base_path,
                &include_patterns,
                &exclude_patterns,
                include_priority
            ));
        }
    }

    #[test]
    fn test_include_exclude_conflict_extension() {
        let base_path = TEST_DIR.path();

        let include_patterns = compile_patterns(&["**/*.py"]);
        let exclude_patterns = compile_patterns(&["**/*.py"]);
        let include_priority = true;

        for file in [
            "lowercase/foo.py",
            "lowercase/bar.py",
            "lowercase/baz.py",
            "uppercase/FOO.py",
            "uppercase/BAR.py",
            "uppercase/BAZ.py",
        ] {
            let path = base_path.join(file);
            assert!(should_include_file(
                &path,
                base_path,
                &include_patterns,
                &exclude_patterns,
                include_priority
            ));
        }

        for file in [
            "lowercase/qux.txt",
            "lowercase/corge.txt",
            "lowercase/grault.txt",
            "uppercase/QUX.txt",
            "uppercase/CORGE.txt",
            "uppercase/GRAULT.txt",
        ] {
            let path = base_path.join(file);
            assert!(!should_include_file(
                &path,
                base_path,
                &include_patterns,
                &exclude_patterns,
                include_priority
            ));
        }
    }

    #[test]
    fn test_should_include_file_no_patterns() {
        let path = Path::new("src/main.rs");
        // In this test, the root is the same as the path itself for simplicity.
        let root = Path::new(".");
        let include_patterns = compile_patterns(&[]);
        let exclude_patterns = compile_patterns(&[]);
        let include_priority = false;
        assert!(should_include_file(
            &path,
            root,
            &include_patterns,
            &exclude_patterns,
            include_priority
        ));
    }

    #[test]
    fn test_should_exclude_file_with_patterns() {
        let path = Path::new("src/main.rs");
        // In this test, the root is the same as the path itself for simplicity.
        let root = Path::new(".");
        let include_patterns = compile_patterns(&[]);
        let exclude_patterns = compile_patterns(&["src/*.rs"]);
        let include_priority = false;
        assert!(!should_include_file(
            &path,
            root,
            &include_patterns,
            &exclude_patterns,
            include_priority
        ));
    }
}
