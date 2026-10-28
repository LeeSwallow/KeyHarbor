//! What a rule reports, and how it is printed.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub rule: &'static str,
    pub file: String,
    pub line: Option<usize>,
    pub message: String,
}

impl Violation {
    pub fn new(rule: &'static str, file: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            rule,
            file: file.into(),
            line: None,
            message: message.into(),
        }
    }

    pub fn at(mut self, line: usize) -> Self {
        self.line = Some(line);
        self
    }
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            Some(line) => write!(
                f,
                "[{}] {}:{}: {}",
                self.rule, self.file, line, self.message
            ),
            None => write!(f, "[{}] {}: {}", self.rule, self.file, self.message),
        }
    }
}

/// One line per violation, for test failure messages.
pub fn render(violations: &[Violation]) -> String {
    violations.iter().map(|v| format!("✗ {v}\n")).collect()
}
