//! Converts [`Statement`] into the specified output format or csv by default.

use parser::{MParserError, Statement};

mod csv;
mod html;
mod json;
mod markdown;
mod xlsx;

/// The file format a [`Statement`] can be exported to.
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
    pub fn requires_output_path(&self) -> bool {
        matches!(self, Self::Xlsx)
    }
}

impl FileFormat {
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Markdown | Self::Md => "md",
            Self::Html => "html",
            Self::Json => "json",
            Self::Xlsx => "xlsx",
        }
    }
}

pub fn run(stmt: Statement, file_format: &FileFormat) -> Result<Vec<u8>, MParserError> {
    let bytes = match file_format {
        FileFormat::Csv => csv::export(stmt)?,
        FileFormat::Markdown | FileFormat::Md => markdown::export(stmt)?,
        FileFormat::Xlsx => xlsx::export(stmt)?,
        FileFormat::Html => html::export(stmt)?,
        FileFormat::Json => json::export(stmt)?,
    };
    Ok(bytes)
}
