//! This module contains UI-related functions for templates,
//! such as prompting for variables and copying to the clipboard.

use std::borrow::Cow;
use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use colored::Colorize;
use handlebars::{
    Handlebars, Template, no_escape,
    template::{Parameter, TemplateElement},
};
#[cfg(feature = "interactive")]
use inquire::Text;
use sha2::{Digest, Sha256};

use crate::common::hash::HashMap;

/// A trait for sources that can provide template content.
pub trait TemplateSource {
    /// Loads the template content and its hash.
    fn load(&self) -> Result<(Cow<'static, str>, String)>;
}

pub struct FileTemplateSource {
    pub candidates: Vec<PathBuf>,
}

impl TemplateSource for FileTemplateSource {
    fn load(&self) -> Result<(Cow<'static, str>, String)> {
        for path in &self.candidates {
            if path.exists() {
                let content = std::fs::read_to_string(path)
                    .with_context(|| format!("Failed to read template file: {}", path.display()))?;
                let hash = hash_content(&content);
                return Ok((content.into(), hash));
            }
        }
        Err(anyhow!("No template file found in candidate paths."))
    }
}

pub struct BuiltinTemplateSource;

impl TemplateSource for BuiltinTemplateSource {
    fn load(&self) -> Result<(Cow<'static, str>, String)> {
        Ok((
            include_str!("../../default_template.hbs").into(),
            "builtin".into(),
        ))
    }
}

/// Hashes a string using SHA256 and returns a hex string.
pub fn hash_content(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hex::encode(hasher.finalize())
}

/// Finds the template to use based on CLI args and filesystem search paths.
/// Returns the template content and its SHA256 hash.
pub fn resolve_template(
    project_path: &Path,
    tpl_arg: &Option<PathBuf>,
) -> Result<(Cow<'static, str>, String)> {
    // 1. Explicit --template flag has highest priority.
    if let Some(path) = tpl_arg {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read template file: {}", path.display()))?;
        let hash = hash_content(&content);
        return Ok((content.into(), hash));
    }

    // 2. Try file-based sources.
    let file_source = FileTemplateSource {
        candidates: vec![
            project_path.join(".code2prompt/template.hbs"),
            dirs::config_dir()
                .unwrap_or_default()
                .join("code2prompt/template.hbs"),
        ],
    };

    if let Ok(result) = file_source.load() {
        return Ok(result);
    }

    // 3. Fallback to built-in default if all file sources fail.
    BuiltinTemplateSource.load()
}

/// A more robust method to extract placeholder names from a template using the Handlebars parser.
pub fn extract_placeholders(template_str: &str) -> Result<Vec<String>> {
    let template = Template::compile(template_str)
        .map_err(|e| anyhow::anyhow!("Failed to parse template for variable extraction: {}", e))?;

    let mut placeholders = HashSet::new(); // Use HashSet to avoid duplicates
    let registered_identifiers: HashSet<&str> = [
        "path",
        "code",
        "git_diff",
        "source_tree",
        "absolute_code_path",
        "files",
        "git_diff_branch",
        "git_log_branch",
    ]
    .iter()
    .cloned()
    .collect();

    for element in &template.elements {
        if let TemplateElement::Expression(expr) = element {
            if let Parameter::Name(name) = &expr.name {
                if !registered_identifiers.contains(name.as_str()) {
                    placeholders.insert(name.clone());
                }
            }
        }
    }

    // Convert HashSet to Vec for the final result
    Ok(placeholders.into_iter().collect())
}

/// Set up the Handlebars template engine.
pub fn handlebars_setup<'a>(template_str: &str, template_name: &str) -> Result<Handlebars<'a>> {
    let mut handlebars = Handlebars::new();
    handlebars.register_escape_fn(no_escape);

    handlebars
        .register_template_string(template_name, template_str)
        .map_err(|e| anyhow::anyhow!("Failed to register template: {}", e))?;

    Ok(handlebars)
}

/// Renders the template with the provided data.
pub fn render_template(
    handlebars: &Handlebars,
    template_name: &str,
    data: &serde_json::Value,
) -> Result<String> {
    let rendered = handlebars
        .render(template_name, data)
        .map_err(|e| anyhow::anyhow!("Failed to render template: {}", e))?;
    Ok(rendered.trim().to_string())
}

/// Writes the rendered template to a specified output file.
pub fn write_to_file(output_path: &str, rendered: &str) -> Result<()> {
    let file = std::fs::File::create(output_path)?;
    let mut writer = std::io::BufWriter::new(file);
    write!(writer, "{rendered}")?;

    #[cfg(feature = "colors")]
    println!(
        "{}{}{} {}",
        "[".bold().white(),
        "✓".bold().green(),
        "]".bold().white(),
        format!("Prompt written to file: {output_path}").green()
    );

    #[cfg(not(feature = "colors"))]
    println!("[✓] {}", format!("Prompt written to file: {}", output_path));

    Ok(())
}

#[cfg(feature = "interactive")]
pub fn prompt_for_variables(
    vars_to_prompt: &[String],
    cached_vars: &HashMap<String, String>,
) -> Result<HashMap<String, String>> {
    let mut new_vars = HashMap::default();
    for var in vars_to_prompt {
        let prompt_text = format!("Enter value for '{var}': ");
        let mut prompt = Text::new(&prompt_text);

        if let Some(cached_val) = cached_vars.get(var) {
            prompt = prompt.with_default(cached_val);
        }

        let answer = prompt
            .with_help_message("This value will be cached for the next run.")
            .prompt()
            .unwrap_or_default();

        new_vars.insert(var.clone(), answer);
    }
    Ok(new_vars)
}
