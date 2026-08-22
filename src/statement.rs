//! Parsing and exporting M-Pesa statements.
//!
//! [`Statement::new`] parses a statement into structured data

use pdf_inspector::{
    PdfRect, TextItem, extractor::extract_text_with_positions_and_rects_with_password,
    tables::detect_tables_from_rects, types::ItemType,
};
use std::{collections::BTreeMap, path::PathBuf};

use crate::{
    MParserError, Meta, Summary, Transactions, summary::summaries, transaction::transactions,
};

/// A parsed M-Pesa statement, holding the customer metadata, the summary
/// table (totals per transaction type), and every individual transaction.
///
/// Construct one with [`Statement::new`] and turn it into a file with
/// [`Statement::export`].
#[derive(Debug)]
pub struct Statement {
    pub meta: Meta,
    pub summary: Vec<Summary>,
    pub transactions: Transactions,
}

impl Statement {
    /// Parses the M-Pesa statement PDF at `statement_path` into a
    /// [`Statement`].
    ///
    /// `password` should be provided if the PDF is password-protected
    ///
    /// # Errors
    ///
    /// Returns [`MParserError`] if the statement can't be read/decrypted, or if
    /// its contents don't match the expected M-Pesa statement layout.
    pub fn new(statement_path: PathBuf, password: Option<&str>) -> Result<Self, MParserError> {
        let (text_items, pdf_rects, _pdf_lines) =
            extract_text_with_positions_and_rects_with_password(statement_path, None, password)?;
        let mut text_items = text_items
            .into_iter()
            .filter(|item| matches!(item.item_type, ItemType::Text));

        Self::validate_statement(&mut text_items)?;

        let meta = Meta::new(&mut text_items)?;

        let tables = Self::get_tables(text_items, pdf_rects);

        if tables.len() < 2 {
            return Err(MParserError::InsufficientData);
        }

        Ok(Self {
            meta,
            summary: summaries(&tables[0])?, // Statement summary is the first table
            transactions: transactions(&tables[1..])?, // The rest of the tables hold transaction details
        })
    }

    /// Groups text items and rects by page, then detects tables on each
    /// page from their rects, returning the cells of every table found
    /// across the whole document, in page order.
    fn get_tables<T: Iterator<Item = TextItem>>(
        text_items: T,
        pdf_rects: Vec<PdfRect>,
    ) -> Vec<Vec<Vec<String>>> {
        let text_items_by_page = text_items.fold(
            BTreeMap::<u32, Vec<TextItem>>::new(),
            |mut text_items_by_page: BTreeMap<u32, Vec<TextItem>>, text_item| {
                text_items_by_page
                    .entry(text_item.page)
                    .or_default()
                    .push(text_item);
                text_items_by_page
            },
        );

        let pdf_rects_by_page = pdf_rects.into_iter().fold(
            BTreeMap::<u32, Vec<PdfRect>>::new(),
            |mut pdf_rects_by_page: BTreeMap<u32, Vec<PdfRect>>, rect| {
                pdf_rects_by_page.entry(rect.page).or_default().push(rect);
                pdf_rects_by_page
            },
        );

        text_items_by_page
            .into_iter()
            .fold(Vec::new(), |mut tables, (page_num, page_items)| {
                tables.extend(
                    detect_tables_from_rects(
                        &page_items,
                        pdf_rects_by_page
                            .get(&page_num)
                            .expect("page number should be a valid key"),
                        page_num,
                    )
                    .0
                    .into_iter()
                    .map(|table| table.cells),
                );
                tables
            })
    }

    /// Validatse the provided pdf
    fn validate_statement<T: Iterator<Item = TextItem>>(
        text_items: &mut T,
    ) -> Result<(), MParserError> {
        let header = text_items
            .nth(1) // skip page number (Page 1 of ..)
            .ok_or(MParserError::InvalidStatement)?
            .text;

        if header != "M-PESA STATEMENT" {
            return Err(MParserError::InvalidStatement);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_pdf() {
        let statement_path = PathBuf::from("statements/statement_1_123456.pdf");
        let password = Some("123456");

        let _ = Statement::new(statement_path, password).unwrap();
    }

    #[test]
    fn invalid_pdf_fails_to_parse() {
        let statement_path = PathBuf::from("statements/chip8ref.pdf");

        let _ = Statement::new(statement_path, None).is_err();
    }
}
