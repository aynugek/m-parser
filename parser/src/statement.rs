//! Parsing and exporting M-Pesa statements.
//!
//! [`Statement::new`] parses a statement into structured data

use crate::{
    Error, Meta, Summary,
    meta::parse_meta,
    pdf::{Chunk, load_fonts, page_text},
    summary::parse_summary,
    transaction::{Transaction, parse_transactions},
};
use lopdf::{Document, LoadOptions};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::io::Read;

/// A parsed M-Pesa statement, holding the customer metadata, the summary
/// table (totals per transaction type), and every individual transaction.
///
/// Construct one with [`Statement::new`].
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct Statement {
    pub meta: Meta,
    pub summary: Vec<Summary>,
    pub transactions: Vec<Transaction>,
}

impl Statement {
    /// Parses an M-Pesa statement PDF read from `source` into a
    /// [`Statement`].
    ///
    /// `password` should be provided if the PDF is password-protected.
    ///
    /// # Errors
    ///
    /// Returns [`Error`] if the statement can't be read/decrypted, or if
    /// its contents don't match the expected M-Pesa statement layout.
    pub fn new<R: Read>(source: R, password: Option<&str>) -> Result<Self, Error> {
        let doc = Document::load_from_with_options(
            source,
            LoadOptions::with_password(password.unwrap_or_default()),
        )?;

        // Fonts are shared by every page; load each object once, then extract
        // and parse pages in parallel. Keeping only the first page's chunks
        // alive avoids holding the whole document's text in memory.
        let fonts = load_fonts(&doc);
        let page_ids = doc.get_pages().into_values().collect::<Vec<_>>();
        let (first_page, transactions) = page_ids
            .par_iter()
            .enumerate()
            .map(|(i, &page_id)| {
                let page = strip_footer(page_text(&doc, page_id, &fonts)?);
                let rows = parse_transactions(&page)?;
                Ok(if i == 0 {
                    (Some(page), rows)
                } else {
                    (None, rows)
                })
            })
            .collect::<Result<Vec<_>, Error>>()?
            .into_iter()
            .fold(
                (None, Vec::new()),
                |(first, mut transactions), (page, rows)| {
                    transactions.extend(rows);
                    (first.or(page), transactions)
                },
            );

        // The metadata and the summary table only appear on the first page.
        let first_page = first_page.ok_or(Error::InsufficientData)?;

        Ok(Self {
            meta: parse_meta(&first_page)?,
            summary: parse_summary(&first_page)?,
            transactions,
        })
    }
}

pub(crate) fn find<'a>(chunks: &'a [Chunk], text: &str) -> Option<&'a Chunk> {
    chunks.iter().find(|c| c.text.trim() == text)
}

pub(crate) fn clean_label(s: &str) -> String {
    s.trim().trim_end_matches(':').trim().to_string()
}

/// Drop the page footer (disclaimer, verification code, contact line). On a full page the
/// last table row sits right above it, so a distance limit alone isn't enough: find the
/// highest footer marker on the page and discard everything at or below it.
fn strip_footer(chunks: Vec<Chunk>) -> Vec<Chunk> {
    let footer_top = chunks
        .iter()
        .filter(|c| {
            let t = c.text.trim_start();
            t.starts_with("Disclaimer") || t.starts_with("Statement Verification Code")
        })
        .map(|c| c.y)
        .fold(f32::NEG_INFINITY, f32::max);

    if footer_top == f32::NEG_INFINITY {
        return chunks; // no footer on this page
    }
    chunks
        .into_iter()
        .filter(|c| c.y > footer_top + 2.0)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn parses_valid_pdf() {
        let statement_path = PathBuf::from("statements/statement_1_123456.pdf");
        let password = Some("123456");

        let statement_bytes = std::fs::read(statement_path).unwrap();

        let _ = Statement::new(statement_bytes.as_slice(), password).unwrap();
    }

    #[test]
    fn invalid_pdf_fails_to_parse() {
        let statement_path = PathBuf::from("statements/chip8ref.pdf");
        let statement_bytes = std::fs::read(statement_path).unwrap();

        assert!(Statement::new(statement_bytes.as_slice(), None).is_err());
    }
}
