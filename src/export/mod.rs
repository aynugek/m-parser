//! Converts [`Statement`] into the specified output format or csv by default.

use std::path::PathBuf;

use m_parser::{MParserError, Statement};

mod csv;
mod html;
mod json;
mod markdown;
mod xlsx;

/// The file format a [`statement::Statement`] can be exported to.
#[derive(argh::FromArgValue, Debug, Default)]
pub enum FileFormat {
    #[default]
    Csv,
    Markdown,
    Md,
    Html,
    Json,
    Xlsx,
}

impl FileFormat {
    /// Returns the conventional file extension if output file
    /// is not specified
    pub fn extension(&self) -> &'static str {
        match self {
            FileFormat::Csv => "csv",
            FileFormat::Markdown | FileFormat::Md => "md",
            FileFormat::Html => "html",
            FileFormat::Json => "json",
            FileFormat::Xlsx => "xlsx",
        }
    }
}

pub fn run(
    statement_path: PathBuf,
    password: Option<String>,
    output_path: PathBuf,
    file_format: FileFormat,
) -> Result<(), MParserError> {
    let stmt = Statement::new(statement_path, password.as_deref())?;
    let bytes = match file_format {
        FileFormat::Csv => csv::export(stmt)?,
        FileFormat::Markdown | FileFormat::Md => markdown::export(stmt)?,
        FileFormat::Html => html::export(stmt)?,
        FileFormat::Json => json::export(stmt)?,
        FileFormat::Xlsx => xlsx::export(stmt)?,
    };

    std::fs::write(&output_path, &bytes)?;
    println!("{} bytes written to {}", bytes.len(), output_path.display());

    Ok(())
}
