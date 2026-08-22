//! The statement summary table: totals paid in/out per transaction type.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{errors::MParserError, money::Money};

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
    type Error = MParserError;

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
            other => return Err(MParserError::InvalidTransactionType(other.to_string())),
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
    /// Returns [`MParserError`] if the transaction type isn't recognized or
    /// either amount can't be parsed.
    pub fn new(row: &[String]) -> Result<Self, MParserError> {
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

/// Parses every row of the statement's summary table (skipping its header
/// row) into [`Summary`] entries.
///
/// # Errors
///
/// Returns [`MParserError`] if any row fails to parse; see [`Summary::new`].
pub fn summaries(summary_table: &[Vec<String>]) -> Result<Vec<Summary>, MParserError> {
    summary_table
        .iter()
        .skip(1) // Skip the header row - | TRANSACTION TYPE | PAID IN | PAID OUT |
        .map(|row| Summary::new(row))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| s.to_string()).collect()
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
    fn summaries_skips_header_row() {
        let table = vec![
            row(&["TRANSACTION TYPE", "PAID IN", "PAID OUT"]),
            row(&["SEND MONEY:", "100.00", ""]),
            row(&["RECEIVED MONEY:", "", "50.00"]),
        ];

        let summaries = summaries(&table).unwrap();

        assert_eq!(summaries.len(), 2);
        assert_eq!(summaries[0].transaction_type().to_string(), "SEND MONEY");
        assert_eq!(
            summaries[1].transaction_type().to_string(),
            "RECEIVED MONEY"
        );
    }
}
