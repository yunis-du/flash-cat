use gpui::Action;
use gpui::KeyBinding;
use schemars::JsonSchema;
use serde::Deserialize;

/// Theme selection actions for the settings menu
#[derive(Clone, Copy, PartialEq, Debug, Deserialize, JsonSchema, Action)]
pub enum ThemeAction {
    /// Light theme mode
    Light,
    /// Dark theme mode
    Dark,
    /// Follow system theme
    System,
}

/// Locale/language selection actions for the settings menu
#[derive(Clone, Copy, PartialEq, Debug, Deserialize, JsonSchema, Action)]
pub enum LocaleAction {
    /// English language
    En,
    /// Chinese language
    Zh,
}

#[derive(Clone, Copy, PartialEq, Debug, Deserialize, JsonSchema, Action)]
pub enum MemuAction {
    Quit,
    About,
}

#[derive(Clone, Copy, PartialEq, Debug, Deserialize, JsonSchema, Action)]
pub enum EditorAction {
    Create,
    Save,
    Reload,
    UpdateTtl,
}

pub fn new_hot_keys() -> Vec<KeyBinding> {
    vec![KeyBinding::new("cmd-q", MemuAction::Quit, None)]
}
