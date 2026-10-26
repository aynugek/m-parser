//! Converts [`Statement`] into the specified output format or csv by default.

use parser::Statement;

pub use crate::errors::ExportError;

mod csv;
mod errors;
mod html;
mod json;
mod markdown;
mod xlsx;

/// The file format a [`Statement`] can be exported to.
#[derive(argh::FromArgValue, Debug, Default, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
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
    /// Every distinct output format, in menu order.
    ///
    /// [`FileFormat::Md`] is an alias for [`FileFormat::Markdown`] and is
    /// therefore omitted.
    pub const ALL: [Self; 5] = [
        Self::Csv,
        Self::Markdown,
        Self::Html,
        Self::Json,
        Self::Xlsx,
    ];

    /// Returns whether the format produces binary output that must be written
    /// to a file rather than a terminal.
    pub fn requires_output_path(self) -> bool {
        matches!(self, Self::Xlsx)
    }

    /// Returns the human-readable name of the format.
    pub fn label(self) -> &'static str {
        match self {
            Self::Csv => "CSV",
            Self::Markdown | Self::Md => "Markdown",
            Self::Html => "HTML",
            Self::Json => "JSON",
            Self::Xlsx => "Excel (XLSX)",
        }
    }

    /// Returns the file extension the format is written with, without the dot.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Markdown | Self::Md => "md",
            Self::Html => "html",
            Self::Json => "json",
            Self::Xlsx => "xlsx",
        }
    }
}

/// Exports `stmt` as `file_format`.
///
/// The statement is borrowed so a caller can keep displaying it and export it
/// more than once.
///
/// # Errors
///
/// Returns [`ExportError`] if the statement cannot be serialized to the
/// requested format.
pub fn run(stmt: &Statement, file_format: FileFormat) -> Result<Vec<u8>, ExportError> {
    let bytes = match file_format {
        FileFormat::Csv => csv::export(stmt)?,
        FileFormat::Markdown | FileFormat::Md => markdown::export(stmt)?,
        FileFormat::Xlsx => xlsx::export(stmt)?,
        FileFormat::Html => html::export(stmt)?,
        FileFormat::Json => json::export(stmt)?,
    };
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::FileFormat;

    #[test]
    fn all_formats_have_a_distinct_label_and_extension() {
        let labels: Vec<_> = FileFormat::ALL.iter().map(|f| f.label()).collect();
        let extensions: Vec<_> = FileFormat::ALL.iter().map(|f| f.extension()).collect();

        let mut unique_labels = labels.clone();
        unique_labels.sort_unstable();
        unique_labels.dedup();
        assert_eq!(labels.len(), unique_labels.len(), "labels: {labels:?}");

        let mut unique_extensions = extensions.clone();
        unique_extensions.sort_unstable();
        unique_extensions.dedup();
        assert_eq!(
            extensions.len(),
            unique_extensions.len(),
            "extensions: {extensions:?}"
        );
    }

    #[test]
    fn markdown_alias_matches_markdown() {
        assert_eq!(FileFormat::Md.extension(), FileFormat::Markdown.extension());
        assert_eq!(FileFormat::Md.label(), FileFormat::Markdown.label());
    }
}
