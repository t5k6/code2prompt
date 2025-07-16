use serde::{Deserialize, Serialize};

use crate::common::hash::HashMap;
use crate::engine::token::TokenizerChoice;
use crate::ui::tui_select::TuiSettings;

#[derive(Default, Serialize, Deserialize, Debug, Clone)]
pub struct TemplateConfig {
    pub defaults: Option<HashMap<String, String>>,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct GuiSection {
    #[serde(default)]
    pub settings: TuiSettings,
}

/// Represents the structure of the `config.toml` file.
/// All fields are optional, so users only need to specify what they want to override.
#[derive(Default, Serialize, Deserialize, Debug, Clone)]
pub struct ConfigFile {
    pub exclude: Option<Vec<String>>,
    pub tokenizer: Option<TokenizerChoice>,
    pub no_codeblock: Option<bool>,
    pub line_numbers: Option<bool>,
    pub no_default_excludes: Option<bool>,
    #[serde(default)]
    // Ensures that if the `template` key is missing, it uses `TemplateConfig::default()`
    pub template: TemplateConfig,
    #[serde(default)]
    pub gui: GuiSection,
}
