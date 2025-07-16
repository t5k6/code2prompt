use code2prompt_tui::engine::token::TokenizerChoice;
use code2prompt_tui::{Code2PromptConfig, Code2PromptSession, ProcessedEntry};
use std::path::PathBuf;

fn create_test_session() -> Code2PromptSession {
    let config = Code2PromptConfig {
        path: PathBuf::from("."),
        include_patterns: vec![],
        exclude_patterns: vec![],
        include_priority: false,
        line_numbers: false,
        absolute_path: false,
        full_directory_tree: false,
        no_codeblock: false,
        tokenizer: TokenizerChoice::Cl100k,
        token_map_enabled: false,
        no_ignore: false,
        hidden: false,
        follow_symlinks: false,
        sort: None,
        cache: false,
    };
    let mut session = Code2PromptSession::new(config).unwrap();
    session.processed_entries = vec![
        ProcessedEntry {
            path: PathBuf::from("./src/main.rs"),
            relative_path: PathBuf::from("src/main.rs"),
            is_file: true,
            code: Some("fn main {}".to_string()),
            extension: Some("rs".to_string()),
            token_count: Some(10),
            mtime: None,
        },
        ProcessedEntry {
            path: PathBuf::from("./src/ui/tui.rs"),
            relative_path: PathBuf::from("src/ui/tui.rs"),
            is_file: true,
            code: Some("...".to_string()),
            extension: Some("rs".to_string()),
            token_count: Some(20),
            mtime: None,
        },
        ProcessedEntry {
            path: PathBuf::from("./docs/guide.md"),
            relative_path: PathBuf::from("docs/guide.md"),
            is_file: true,
            code: Some("...".to_string()),
            extension: Some("md".to_string()),
            token_count: Some(30),
            mtime: None,
        },
        ProcessedEntry {
            path: PathBuf::from("./Cargo.toml"),
            relative_path: PathBuf::from("Cargo.toml"),
            is_file: true,
            code: Some("...".to_string()),
            extension: Some("toml".to_string()),
            token_count: Some(5),
            mtime: None,
        },
    ];
    session
}

#[test]
fn test_filter_by_extension() {
    let mut session = create_test_session();
    let sel_exts = vec!["rs".to_string()];
    let sel_paths: Vec<String> = vec![];
    code2prompt_tui::ui::tui_select::filter_session_entries(&mut session, &sel_exts, &sel_paths);
    assert_eq!(session.processed_entries.len(), 2);
    assert!(
        session
            .processed_entries
            .iter()
            .all(|e| e.extension.as_deref() == Some("rs"))
    );
}

#[test]
fn test_filter_by_path() {
    let mut session = create_test_session();
    let sel_exts: Vec<String> = vec![];
    let sel_paths = vec!["src".to_string()];
    code2prompt_tui::ui::tui_select::filter_session_entries(&mut session, &sel_exts, &sel_paths);
    assert_eq!(session.processed_entries.len(), 2);
    assert!(
        session
            .processed_entries
            .iter()
            .all(|e| e.relative_path.starts_with("src"))
    );
}

#[test]
fn test_filter_by_extension_and_path() {
    let mut session = create_test_session();
    let sel_exts = vec!["rs".to_string()];
    let sel_paths = vec!["src/ui".to_string()];
    code2prompt_tui::ui::tui_select::filter_session_entries(&mut session, &sel_exts, &sel_paths);
    assert_eq!(session.processed_entries.len(), 1);
    assert_eq!(
        session.processed_entries[0].relative_path,
        PathBuf::from("src/ui/tui.rs")
    );
}

#[test]
fn test_filter_with_no_matches() {
    let mut session = create_test_session();
    let sel_exts = vec!["java".to_string()];
    let sel_paths: Vec<String> = vec![];
    code2prompt_tui::ui::tui_select::filter_session_entries(&mut session, &sel_exts, &sel_paths);
    assert!(session.processed_entries.is_empty());
}
