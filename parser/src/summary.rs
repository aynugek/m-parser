//! The statement summary table: totals paid in/out per transaction type.

use crate::{
    errors::Error,
    money::Money,
    pdf::Chunk,
    statement::{clean_label, find},
};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Column headers of the statement summary table
#[derive(Debug)]
pub enum SummaryHeader {
    /// The `"Transaction Type"` column.
    TransactionType,
    /// The `"Paid In"` column.
    PaidIn,
    /// The `"Paid Out"` column.
    PaidOut,
}

impl fmt::Display for SummaryHeader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SummaryHeader::TransactionType => f.write_str("Transaction Type"),
            SummaryHeader::PaidIn => f.write_str("Paid In"),
            SummaryHeader::PaidOut => f.write_str("Paid Out"),
        }
    }
}

/// Categories of transaction.
#[derive(Debug, Deserialize, Serialize)]
pub enum TransactionType {
    SendMoney,
    ReceivedMoney,
    AgentDeposit,
    AgentWithdrawal,
    PayBill,
    BuyGoods,
    Others,
    Total,
}

impl fmt::Display for TransactionType {
    /// The type's display text, exactly as printed in the statement PDF.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransactionType::SendMoney => f.write_str("SEND MONEY"),
            TransactionType::ReceivedMoney => f.write_str("RECEIVED MONEY"),
            TransactionType::AgentDeposit => f.write_str("AGENT DEPOSIT"),
            TransactionType::AgentWithdrawal => f.write_str("AGENT WITHDRAWAL"),
            TransactionType::PayBill => f.write_str("LIPA NA M-PESA (PAYBILL)"),
            TransactionType::BuyGoods => f.write_str("LIPA NA M-PESA (BUY GOODS)"),
            TransactionType::Others => f.write_str("OTHERS"),
            TransactionType::Total => f.write_str("TOTAL"),
        }
    }
}

impl TryFrom<&str> for TransactionType {
    type Error = Error;

    /// Parses a transaction type from its display text
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        Ok(match s {
            "SEND MONEY" => TransactionType::SendMoney,
            "RECEIVED MONEY" => TransactionType::ReceivedMoney,
            "AGENT DEPOSIT" => TransactionType::AgentDeposit,
            "AGENT WITHDRAWAL" => TransactionType::AgentWithdrawal,
            "LIPA NA M-PESA (PAYBILL)" => TransactionType::PayBill,
            "LIPA NA M-PESA (BUY GOODS)" => TransactionType::BuyGoods,
            "OTHERS" => TransactionType::Others,
            "TOTAL" => TransactionType::Total,
            other => return Err(Error::InvalidTransactionType(other.to_string())),
        })
    }
}

/// A single row of the statement summary table for one [`TransactionType`].
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct Summary {
    transaction_type: TransactionType,
    paid_in: Money,
    paid_out: Money,
}

impl Summary {
    /// Parses a summary row, where `row` is `[transaction type, paid in,
    /// paid out]` as extracted from the statement's summary table.
    ///
    /// # Errors
    ///
    /// Returns [`Error`] if the transaction type isn't recognized or
    /// either amount can't be parsed.
    pub fn new(row: &[String]) -> Result<Self, Error> {
        Ok(Self {
            transaction_type: TransactionType::try_from(row[0].trim_end_matches(':'))?,
            paid_in: Money::try_from(row[1].as_ref())?,
            paid_out: Money::try_from(row[2].as_ref())?,
        })
    }

    /// The transaction type's display text.
    pub fn transaction_type(&self) -> &TransactionType {
        &self.transaction_type
    }

    /// Total amount paid in for this transaction type.
    pub fn paid_in(&self) -> &Money {
        &self.paid_in
    }

    /// Total amount paid out for this transaction type.
    pub fn paid_out(&self) -> &Money {
        &self.paid_out
    }
}

/// Parses every row of the statement's summary table into [`Summary`] entries.
///
/// # Errors
///
/// Returns [`Error::InvalidSummary`] if the SUMMARY or DETAILED STATEMENT
/// headings can't be located, or another [`Error`] if any row fails to
/// parse; see [`Summary::new`].
pub fn parse_summary(chunks: &[Chunk]) -> Result<Vec<Summary>, Error> {
    // The SUMMARY block: [transaction type, paid in, paid out], including the TOTAL row.
    // It lies between the "TRANSACTION TYPE" header and the "DETAILED STATEMENT" heading.
    // Paid In / Paid Out are split at the midpoint between their header x positions.
    let (Some(top), Some(bottom)) = (find(chunks, "SUMMARY"), find(chunks, "DETAILED STATEMENT"))
    else {
        return Err(Error::InvalidSummary);
    };
    let header_y = find(chunks, "TRANSACTION TYPE")
        .map(|c| c.y)
        .unwrap_or(top.y - 8.0);
    let mid = match (find(chunks, "PAID IN"), find(chunks, "PAID OUT")) {
        (Some(a), Some(b)) => (a.x + b.x) / 2.0,
        _ => 440.0,
    };
    let in_band = |c: &Chunk| c.y < header_y - 2.0 && c.y > bottom.y + 2.0;

    let mut anchors: Vec<&Chunk> = chunks
        .iter()
        .filter(|c| c.x < 100.0 && in_band(c) && c.text.trim_end().ends_with(':'))
        .collect();
    anchors.sort_by(|a, b| b.y.partial_cmp(&a.y).unwrap());

    let mut rows: Vec<[String; 3]> = anchors
        .iter()
        .map(|a| [clean_label(&a.text), String::new(), String::new()])
        .collect();

    for c in chunks.iter().filter(|c| c.x >= 300.0 && in_band(c)) {
        let nearest = anchors
            .iter()
            .enumerate()
            .map(|(i, a)| (i, (a.y - c.y).abs()))
            .filter(|(_, d)| *d <= 4.0)
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        if let Some((i, _)) = nearest {
            rows[i][if c.x < mid { 1 } else { 2 }] = c.text.trim().to_string();
        }
    }

    rows.iter().map(|row| Summary::new(row)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| s.to_string()).collect()
    }

    fn chunk(text: &str, x: f32, y: f32) -> Chunk {
        Chunk {
            x,
            y,
            text: text.to_owned(),
        }
    }

    #[test]
    fn valid_transaction_type_parses() {
        assert!(matches!(
            TransactionType::try_from("SEND MONEY"),
            Ok(TransactionType::SendMoney)
        ));
    }

    #[test]
    fn invalid_transaction_type_does_not_parse() {
        assert!(TransactionType::try_from("ABCDE").is_err());
    }

    #[test]
    fn valid_summary_row_parses() {
        let summary = Summary::new(&row(&["SEND MONEY:", "1,234.50", ""])).unwrap();
        assert_eq!(summary.transaction_type().to_string(), "SEND MONEY");
        assert_eq!(summary.paid_in().to_string(), "1,234.50");
        assert_eq!(summary.paid_out().to_string(), "");
    }

    #[test]
    fn invalid_summary_row_does_not_parse() {
        assert!(Summary::new(&row(&["ABCDE", "0", "0"])).is_err());
    }

    #[test]
    fn parse_summary_parses_rows_between_header_and_detailed_statement() {
        let chunks = vec![
            chunk("SUMMARY", 50.0, 620.0),
            chunk("TRANSACTION TYPE", 50.0, 600.0),
            chunk("PAID IN", 350.0, 600.0),
            chunk("PAID OUT", 500.0, 600.0),
            chunk("SEND MONEY:", 50.0, 580.0),
            chunk("100.00", 350.0, 580.0),
            chunk("RECEIVED MONEY:", 50.0, 560.0),
            chunk("50.00", 500.0, 560.0),
            chunk("DETAILED STATEMENT", 50.0, 400.0),
        ];

        let summaries = parse_summary(&chunks).unwrap();

        assert_eq!(summaries.len(), 2);
        assert_eq!(summaries[0].transaction_type().to_string(), "SEND MONEY");
        assert_eq!(summaries[0].paid_in().to_string(), "100.00");
        assert_eq!(
            summaries[1].transaction_type().to_string(),
            "RECEIVED MONEY"
        );
        assert_eq!(summaries[1].paid_out().to_string(), "50.00");
    }

    #[test]
    fn summary_without_summary_block_is_invalid() {
        assert!(matches!(parse_summary(&[]), Err(Error::InvalidSummary)));
    }
}
