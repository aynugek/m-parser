//! Errors surfaced by the `mparser-cli` binary.

use std::io;

/// Errors that can occur while running the CLI.
#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    /// The statement PDF couldn't be read or parsed.
    #[error(transparent)]
    Parser(#[from] parser::MParserError),

    /// The statement or export couldn't be read from or written to an output
    #[error(transparent)]
    Io(#[from] io::Error),

    /// The xlsx output format option errors
    #[error(transparent)]
    XlsxWriter(#[from] rust_xlsxwriter::XlsxError),

    /// Errors when exporting to json
    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),
}
