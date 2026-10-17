//! Errors surfaced by the `mparser-cli` binary.

use std::io;

/// Errors that can occur while running the CLI.
#[derive(Debug, thiserror::Error)]
pub enum CliError {
    /// The statement PDF couldn't be read or parsed.
    #[error(transparent)]
    Parser(#[from] parser::MParserError),

    /// The statement or export couldn't be read from or written to an output
    #[error(transparent)]
    Io(#[from] io::Error),

    /// The chosen format can't be written to stdout and needs an explicit
    /// output path.
    #[error("output path required for \"{0}\" format")]
    OutputPathRequired(String),

    /// Errors encountered when trying to export statements
    #[error(transparent)]
    Export(#[from] export::ExportError),
}
