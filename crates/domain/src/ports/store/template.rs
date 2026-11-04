//! Credential templates (R-48), kept with the settings in the config context.

use crate::vault::{CredentialTemplate, TemplateName};
use anyhow::Result;

/// Stores credential templates. A template holds no value.
pub trait TemplateStore {
    /// Every template, by name.
    fn template_list(&self) -> Result<Vec<CredentialTemplate>>;
    /// Adds `template`; false (and nothing changed) if one of that name exists.
    fn template_add(&self, template: &CredentialTemplate) -> Result<bool>;
    /// Removes the template `name`; false if there was none.
    fn template_remove(&self, name: &TemplateName) -> Result<bool>;
}
