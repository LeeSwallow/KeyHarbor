//! Masking on the way out (03 §3.7): every result and error message passes the masker exactly
//! once before it leaves the supervisor. Whatever cannot be masked is withheld, never sent.

use super::Guard;
use anyhow::{Result, anyhow, bail};
use serde_json::Value;

/// The refusal when the values cannot be loaded or the masker cannot be built.
const RESPONSE_WITHHELD: &str = "response withheld: vault values could not be loaded for masking";
/// A masked string that came back as something else (it cannot: the masker keeps shapes).
const OPERATION_FAILED: &str = "operation failed";

impl Guard {
    /// Every result before it leaves the supervisor: each vault value becomes its placeholder.
    /// If the values cannot be loaded or the masker cannot be built, the response is withheld,
    /// never sent unmasked.
    pub fn redact(&self, value: Value) -> Result<Value> {
        let masked = self.with_vault(|loaded| -> Result<Value> {
            let Some(loaded) = loaded else {
                return Ok(value);
            };
            Ok(loaded.masker()?.redact_json(value))
        });
        match masked {
            Ok(Ok(value)) => Ok(value),
            _ => bail!(RESPONSE_WITHHELD),
        }
    }

    /// Text before it leaves the supervisor, masked like [`Guard::redact`].
    pub fn redact_text(&self, text: String) -> Result<String> {
        match self.redact(Value::String(text))? {
            Value::String(text) => Ok(text),
            _ => bail!(OPERATION_FAILED),
        }
    }

    /// A whole outcome, masked exactly once: the value, or the error's message.
    pub fn redact_outcome(&self, outcome: Result<Value>) -> Result<Value> {
        match outcome {
            Ok(value) => self.redact(value),
            Err(error) => Err(self.redact_failure(error)),
        }
    }

    /// An outcome whose value holds no page text ([`FillOutcome`], `()`): only an error's
    /// message is masked.
    ///
    /// [`FillOutcome`]: keyharbor_domain::ports::browser::FillOutcome
    pub fn redact_error<T>(&self, outcome: Result<T>) -> Result<T> {
        outcome.map_err(|error| self.redact_failure(error))
    }

    fn redact_failure(&self, error: anyhow::Error) -> anyhow::Error {
        match self.redact_text(format!("{error:#}")) {
            Ok(message) => anyhow!(message),
            Err(withheld) => withheld,
        }
    }
}
