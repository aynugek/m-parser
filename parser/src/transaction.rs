//! The detailed transaction table holding individual transaction details

use std::fmt;

use jiff::civil::{Date, DateTime, Time};
use serde::{Deserialize, Serialize};

use crate::{errors::Error, money::Money};

const COMPLETION_TIME_STR: &str = "%Y-%m-%d %H:%M:%S";

/// A list of individual transactions from a statement.
pub type Transactions = Vec<Transaction>;

/// Column headers of the detailed transaction tables, as they appear in the
/// source PDF.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TransactionHeader {
    /// The `"Receipt No."` column.
    ReceiptNo,
    /// The `"Completion Time"` column.
    CompletionTime,
    /// The `"Details"` column.
    Details,
    /// The `"Status"` column.
    TransactionStatus,
    /// The `"Paid In"` column.
    PaidIn,
    /// The `"Withdrawn"` column.
    Withdrawn,
    /// The `"Balance"` column.
    Balance,
}

impl fmt::Display for TransactionHeader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransactionHeader::ReceiptNo => f.write_str("Receipt No."),
            TransactionHeader::CompletionTime => f.write_str("Completion Time"),
            TransactionHeader::Details => f.write_str("Details"),
            TransactionHeader::TransactionStatus => f.write_str("Status"),
            TransactionHeader::PaidIn => f.write_str("Paid In"),
            TransactionHeader::Withdrawn => f.write_str("Withdrawn"),
            TransactionHeader::Balance => f.write_str("Balance"),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TransactionCompletionTime(DateTime);

impl fmt::Display for TransactionCompletionTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.strftime(COMPLETION_TIME_STR),)
    }
}

impl TryFrom<&str> for TransactionCompletionTime {
    type Error = Error;

    /// Parses the fixed-width `YYYY-MM-DD HH:MM:SS` form without parsing a format
    /// string. Returns `Error::InvalidCompletionTime` for any other shape.
    fn try_from(completion_time: &str) -> Result<Self, Self::Error> {
        let b = completion_time.as_bytes();
        if b.len() != 19
            || b[4] != b'-'
            || b[7] != b'-'
            || b[10] != b' '
            || b[13] != b':'
            || b[16] != b':'
        {
            return Err(Error::InvalidCompletionTime);
        }

        let year = digits(&b[0..4]).ok_or(Error::InvalidCompletionTime)?;
        let month = digits(&b[5..7]).ok_or(Error::InvalidCompletionTime)?;
        let day = digits(&b[8..10]).ok_or(Error::InvalidCompletionTime)?;
        let hour = digits(&b[11..13]).ok_or(Error::InvalidCompletionTime)?;
        let minute = digits(&b[14..16]).ok_or(Error::InvalidCompletionTime)?;
        let second = digits(&b[17..19]).ok_or(Error::InvalidCompletionTime)?;

        let date = Date::new(year as i16, month as i8, day as i8)?;
        let time = Time::new(hour as i8, minute as i8, second as i8, 0)?;

        Ok(TransactionCompletionTime(date.at(
            time.hour(),
            time.minute(),
            time.second(),
            time.subsec_nanosecond(),
        )))
    }
}

/// Parses an ASCII decimal run, rejecting anything else.
fn digits(b: &[u8]) -> Option<u32> {
    b.iter().try_fold(0u32, |acc, &b| {
        if b.is_ascii_digit() {
            Some(acc * 10 + u32::from(b - b'0'))
        } else {
            None
        }
    })
}

/// A single transaction listed in a statement's detailed transaction table.
#[derive(Debug, Deserialize, Serialize)]
pub struct Transaction {
    receipt_no: String,
    completion_time: TransactionCompletionTime,
    details: String,
    transaction_status: TransactionStatus,
    paid_in: Money,
    withdrawn: Money,
    balance: Money,
}

impl Transaction {
    /// Parses a transaction row, where `row` is `[receipt no, completion
    /// time, details, status, paid in, withdrawn, balance]` as extracted
    /// from a statement's detailed transaction table.
    ///
    /// # Errors
    ///
    /// Returns [`Error`] if parsing any field fails
    pub fn new(row: &[String]) -> Result<Self, Error> {
        Ok(Self {
            receipt_no: row[0].to_owned(),
            completion_time: row[1].as_str().try_into()?,
            details: row[2].to_owned(),
            transaction_status: TransactionStatus::try_from(row[3].as_ref())?,
            paid_in: row[4].as_str().try_into()?,
            withdrawn: row[5].as_str().try_into()?,
            balance: row[6].as_str().try_into()?,
        })
    }

    /// The M-Pesa receipt/transaction reference number.
    pub fn receipt_no(&self) -> &str {
        &self.receipt_no
    }
    /// When the transaction completed.
    pub fn completion_time(&self) -> &TransactionCompletionTime {
        &self.completion_time
    }
    /// A free-text description of the transaction.
    pub fn details(&self) -> &str {
        &self.details
    }
    /// The transaction's status display text (e.g. `"Completed"`).
    pub fn transaction_status(&self) -> &TransactionStatus {
        &self.transaction_status
    }
    /// Amount paid into the account by this transaction.
    pub fn paid_in(&self) -> &Money {
        &self.paid_in
    }
    /// Amount withdrawn from the account by this transaction.
    pub fn withdrawn(&self) -> &Money {
        &self.withdrawn
    }
    /// Account balance after this transaction.
    pub fn balance(&self) -> &Money {
        &self.balance
    }
}

/// The completion status of a transaction.
#[derive(Debug, Default, Deserialize, Serialize)]
pub enum TransactionStatus {
    #[default]
    Completed,
}

impl fmt::Display for TransactionStatus {
    /// The status's display text.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransactionStatus::Completed => f.write_str("Completed"),
        }
    }
}

impl TryFrom<&str> for TransactionStatus {
    type Error = Error;

    /// Parses a transaction status from its display text (e.g.
    /// `"Completed"`).
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        Ok(match s {
            "Completed" => TransactionStatus::Completed,
            other => return Err(Error::InvalidTransactionStatus(other.to_string())),
        })
    }
}

/// Parses every row of the statement's detailed transaction tables (each
/// with its own header row, which is skipped) into [`Transaction`]s.
///
/// # Errors
///
/// Returns [`Error`] if any row fails to parse; see
/// [`Transaction::new`].
pub fn transactions(detailed_summary_table: &[Vec<Vec<String>>]) -> Result<Transactions, Error> {
    detailed_summary_table
        .iter()
        .flat_map(|rows| {
            rows.iter()
                .skip(1) // Skip header row - | Receipt No. | Completion Time | Details | Transaction Status | Paid In | Withdrawn | Balance |
                .map(|row| Transaction::new(row))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn transaction_status_parses_completed() {
        assert_eq!(
            TransactionStatus::try_from("Completed")
                .unwrap()
                .to_string(),
            "Completed"
        );
    }

    #[test]
    fn transaction_status_rejects_unknown_values() {
        assert!(TransactionStatus::try_from("abcde").is_err());
    }

    #[test]
    fn transaction_new_parses_a_row() {
        let t = Transaction::new(&row(&[
            "ABC123XYZ",
            "2024-01-15 10:30:00",
            "Pay Bill to Acme Ltd.",
            "Completed",
            "1,000.00",
            "",
            "4,321.00",
        ]))
        .unwrap();

        assert_eq!(t.receipt_no(), "ABC123XYZ");
        assert_eq!(t.completion_time().to_string(), "2024-01-15 10:30:00");
        assert_eq!(t.details(), "Pay Bill to Acme Ltd.");
        assert_eq!(t.transaction_status().to_string(), "Completed");
        assert_eq!(t.paid_in().to_string(), "1,000.00");
        assert_eq!(t.withdrawn().to_string(), "");
        assert_eq!(t.balance().to_string(), "4,321.00");
    }

    #[test]
    fn transaction_new_rejects_bad_completion_time() {
        assert!(
            Transaction::new(&row(&[
                "ABC123XYZ",
                "abcde",
                "Details",
                "Completed",
                "0",
                "0",
                "0",
            ]))
            .is_err()
        );
    }

    #[test]
    fn transactions_skips_each_tables_header_row() {
        let tables = vec![
            vec![
                row(&[
                    "Receipt No.",
                    "Completion Time",
                    "Details",
                    "Status",
                    "Paid In",
                    "Withdrawn",
                    "Balance",
                ]),
                row(&[
                    "A1",
                    "2024-01-01 08:00:00",
                    "First",
                    "Completed",
                    "100.00",
                    "",
                    "100.00",
                ]),
            ],
            vec![
                row(&[
                    "Receipt No.",
                    "Completion Time",
                    "Details",
                    "Status",
                    "Paid In",
                    "Withdrawn",
                    "Balance",
                ]),
                row(&[
                    "B2",
                    "2024-01-02 09:00:00",
                    "Second",
                    "Completed",
                    "",
                    "50.00",
                    "50.00",
                ]),
            ],
        ];

        let txns = transactions(&tables).unwrap();

        assert_eq!(txns.len(), 2);
        assert_eq!(txns[0].receipt_no(), "A1");
        assert_eq!(txns[1].receipt_no(), "B2");
    }
}
