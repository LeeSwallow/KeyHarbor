//! Credential templates (R-48), kept with the settings in the config context.

use super::StoreError;
use crate::vault::{CredentialTemplate, TemplateName};

type Result<T> = std::result::Result<T, StoreError>;

/// Stores credential templates. A template holds no value.
pub trait TemplateStore {
    /// Every template, by name.
    fn template_list(&self) -> Result<Vec<CredentialTemplate>>;
    /// Adds `template`; false (and nothing changed) if one of that name exists.
    fn template_add(&self, template: &CredentialTemplate) -> Result<bool>;
    /// Removes the template `name`; false if there was none.
    fn template_remove(&self, name: &TemplateName) -> Result<bool>;
}
