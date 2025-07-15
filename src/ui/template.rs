// src/template.rs

//! This module contains UI-related functions for templates,
//! such as prompting for variables and copying to the clipboard.

use anyhow::Result;
#[cfg(feature = "colors")]
use colored::*;
use handlebars::{no_escape, Handlebars};
#[cfg(feature = "interactive")]
use inquire::Text;
use regex::Regex;
use std::io::Write;

/// Set up the Handlebars template engine.
pub fn handlebars_setup(template_str: &str, template_name: &str) -> Result<Handlebars<'static>> {
    let mut handlebars = Handlebars::new();
    handlebars.register_escape_fn(no_escape);

    handlebars
        .register_template_string(template_name, template_str)
        .map_err(|e| anyhow::anyhow!("Failed to register template: {}", e))?;

    Ok(handlebars)
}

/// Extracts the undefined variables from the template string.
pub fn extract_undefined_variables(template: &str) -> Vec<String> {
    let registered_identifiers = ["path", "code", "git_diff"];
    let re = Regex::new(r"\{\{\s*(?P<var>[a-zA-Z_][a-zA-Z_0-9]*)\s*\}\}").unwrap();
    re.captures_iter(template)
        .map(|cap| cap["var"].to_string())
        .filter(|var| !registered_identifiers.contains(&var.as_str()))
        .collect()
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

/// Handles user-defined variables in the template and adds them to the data.
pub fn handle_undefined_variables(
    data: &mut serde_json::Value,
    template_content: &str,
    no_interactive: bool,
) -> Result<()> {
    let undefined_variables: Vec<_> = extract_undefined_variables(template_content)
        .into_iter()
        .filter(|var| !data.as_object().unwrap().contains_key(var))
        .collect();

    if !undefined_variables.is_empty() && no_interactive {
        return Err(anyhow::anyhow!(
            "Template requires user-defined variables, but running in non-interactive mode. Missing variables: {:?}",
            undefined_variables
        ));
    }

    if !undefined_variables.is_empty() {
        #[cfg(not(feature = "interactive"))]
        return Err(anyhow::anyhow!(
            "Template requires user-defined variables, but the binary was compiled without the `interactive` feature."
        ));
    }

    #[cfg(feature = "interactive")]
    {
        let mut user_defined_vars = serde_json::Map::new();
        for var in undefined_variables.iter() {
            let prompt = format!("Enter value for '{}': ", var);
            let answer = Text::new(&prompt)
                .with_help_message("Fill user defined variable in template")
                .prompt()
                .unwrap_or_default();
            user_defined_vars.insert(var.clone(), serde_json::Value::String(answer));
        }

        if let Some(obj) = data.as_object_mut() {
            for (key, value) in user_defined_vars {
                obj.insert(key, value);
            }
        }
    }

    Ok(())
}

/// Handles user-defined variables by prompting the user.
#[cfg(feature = "interactive")]
pub fn prompt_for_undefined_variables(
    data: &mut serde_json::Value,
    template_content: &str,
) -> Result<()> {
    let undefined_variables: Vec<_> = extract_undefined_variables(template_content)
        .into_iter()
        .filter(|var| !data.as_object().unwrap().contains_key(var))
        .collect();

    if !undefined_variables.is_empty() {
        let mut user_defined_vars = serde_json::Map::new();
        for var in undefined_variables.iter() {
            let prompt = format!("Enter value for '{}': ", var);
            let answer = Text::new(&prompt)
                .with_help_message("Fill user defined variable in template")
                .prompt()
                .unwrap_or_default();
            user_defined_vars.insert(var.clone(), serde_json::Value::String(answer));
        }

        if let Some(obj) = data.as_object_mut() {
            for (key, value) in user_defined_vars {
                obj.insert(key, value);
            }
        }
    }
    Ok(())
}

/// Writes the rendered template to a specified output file.
pub fn write_to_file(output_path: &str, rendered: &str) -> Result<()> {
    let file = std::fs::File::create(output_path)?;
    let mut writer = std::io::BufWriter::new(file);
    write!(writer, "{}", rendered)?;

    #[cfg(feature = "colors")]
    println!(
        "{}{}{} {}",
        "[".bold().white(),
        "✓".bold().green(),
        "]".bold().white(),
        format!("Prompt written to file: {}", output_path).green()
    );

    #[cfg(not(feature = "colors"))]
    println!("[✓] {}", format!("Prompt written to file: {}", output_path));

    Ok(())
}
