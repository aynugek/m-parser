//! Application errors shown to the user instead of a panic.

use std::path::PathBuf;

/// A failure while opening, displaying or exporting a statement.
#[derive(Debug, thiserror::Error)]
pub enum GuiError {
    /// The selected PDF could not be read from disk.
    #[error("could not read {}: {source}", path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// The PDF could not be parsed as an M-Pesa statement.
    #[error(transparent)]
    Parse(#[from] parser::MParserError),

    /// The statement could not be converted to the requested format.
    #[error(transparent)]
    Export(#[from] export::ExportError),

    /// The exported file could not be written to disk.
    #[error("could not write {}: {source}", path.display())]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

impl GuiError {
    /// Returns `true` when the PDF is encrypted and needs a password, including
    /// when the supplied password was rejected.
    pub fn is_password_error(&self) -> bool {
        matches!(self, Self::Parse(error) if error.is_password_error())
    }
}

/// The message shown when the statement needs a password that was not supplied.
pub const PASSWORD_REQUIRED_MESSAGE: &str =
    "This statement is password-protected. Enter its password.";

/// The message shown when the supplied password was rejected.
pub const PASSWORD_INCORRECT_MESSAGE: &str = "That password didn't work. Try again.";
