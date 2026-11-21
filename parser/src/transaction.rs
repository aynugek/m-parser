//! The detailed transaction table holding individual transaction details

use std::fmt;

use crate::{errors::Error, money::Money, pdf::Chunk};
use jiff::civil::{Date, DateTime, Time};
use serde::{Deserialize, Serialize};

const COMPLETION_TIME_STR: &str = "%Y-%m-%d %H:%M:%S";

/// Column left edges (PDF points, A4 = 595 x 842); the last value is the table's right edge.
/// Columns: Receipt, Time, Details, Status, Paid In, Withdrawn, Balance.
/// Checked against the sample statement: cells start at x = 38 / 108 / 177 / 282 /
/// 391..404 / 464..472 / 530..544.
const DEFAULT_COLS: [f32; 8] = [35.0, 106.0, 175.0, 280.0, 336.0, 421.0, 490.0, 565.0];

/// Max distance (points) between a row's receipt line and its last wrapped line.
const MAX_ROW_HEIGHT: f32 = 70.0;

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
        if row.len() < 7 {
            return Err(Error::InvalidStatement);
        }
        Self::from_row(std::array::from_fn(|i| row[i].clone()))
    }

    /// Parses a row that is already owned, moving its cell strings.
    fn from_row(
        [
            receipt_no,
            completion_time,
            details,
            transaction_status,
            paid_in,
            withdrawn,
            balance,
        ]: [String; 7],
    ) -> Result<Self, Error> {
        let completion_time = completion_time.as_str().try_into()?;
        let transaction_status = TransactionStatus::try_from(transaction_status.as_str())?;
        Ok(Self {
            receipt_no,
            completion_time,
            details,
            transaction_status,
            paid_in: paid_in.as_str().try_into()?,
            withdrawn: withdrawn.as_str().try_into()?,
            balance: balance.as_str().try_into()?,
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

/// The table column containing `x`, if any.
fn col_of(cols: &[f32; 8], x: f32) -> Option<usize> {
    (0..7).find(|&i| x >= cols[i] - 1.0 && x < cols[i + 1])
}

/// Whether `s` looks like an M-Pesa receipt number (10 uppercase letters/digits).
fn is_receipt(s: &str) -> bool {
    let s = s.trim();
    s.len() == 10
        && s.chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
}

/// Parses every transaction row of the statement's detailed transaction
/// table into [`Transaction`]s. Rows are anchored on the receipt numbers in
/// the first column; wrapped lines are joined onto their owning row, and
/// header and footer text is ignored.
///
/// # Errors
///
/// Returns [`Error`] if any row fails to parse; see
/// [`Transaction::new`].
pub fn parse_transactions(chunks: &[Chunk]) -> Result<Vec<Transaction>, Error> {
    let cols = &DEFAULT_COLS;

    // Row anchors = receipt numbers in column 0, top to bottom.
    let mut anchors: Vec<&Chunk> = chunks
        .iter()
        .filter(|c| col_of(cols, c.x) == Some(0) && is_receipt(&c.text))
        .collect();
    anchors.sort_by(|a, b| b.y.total_cmp(&a.y));

    let mut rows = vec![<[String; 7]>::default(); anchors.len()];

    // Reading order: top -> bottom, left -> right, so wrapped lines concatenate correctly.
    let mut sorted: Vec<&Chunk> = chunks.iter().collect();
    if !sorted.is_sorted_by(|a, b| reading_order(a, b).is_le()) {
        sorted.sort_by(|a, b| reading_order(a, b));
    }

    for c in sorted {
        let Some(col) = col_of(cols, c.x) else {
            continue;
        };
        let Some(row) = owner_index(&anchors, c) else {
            continue;
        };

        let cell = &mut rows[row][col];
        if !cell.is_empty() {
            cell.push(' ');
        }
        cell.push_str(c.text.trim());
    }

    rows.into_iter().map(Transaction::from_row).collect()
}

/// The row whose receipt anchor owns `chunk`.
///
/// Owning row = closest anchor at or above the chunk (small tolerance upward),
/// but not further than [`MAX_ROW_HEIGHT`] so the footer isn't glued to the last
/// row. `anchors` is sorted by descending y, so candidates form a prefix.
fn owner_index(anchors: &[&Chunk], chunk: &Chunk) -> Option<usize> {
    let end = anchors.partition_point(|a| a.y >= chunk.y - 2.0);
    let last = end.checked_sub(1)?;

    // Ties on y: the old linear scan picked the first anchor, keep that.
    let y = anchors[last].y;
    let first = anchors[..end].partition_point(|a| a.y > y);
    (anchors[first].y - chunk.y <= MAX_ROW_HEIGHT).then_some(first)
}

/// Reading order: descending y, then ascending x.
fn reading_order(a: &Chunk, b: &Chunk) -> std::cmp::Ordering {
    b.y.total_cmp(&a.y).then_with(|| a.x.total_cmp(&b.x))
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

    /// Builds one table row's chunks at height `y`, one per column.
    fn table_row(cells: [&str; 7], y: f32) -> Vec<Chunk> {
        let xs = [38.0, 108.0, 177.0, 282.0, 400.0, 470.0, 535.0];
        cells
            .iter()
            .zip(xs)
            .map(|(text, x)| chunk(text, x, y))
            .collect()
    }

    #[test]
    fn parse_transactions_parses_rows_and_ignores_header() {
        let mut chunks = table_row(
            [
                "Receipt No.",
                "Completion Time",
                "Details",
                "Status",
                "Paid In",
                "Withdrawn",
                "Balance",
            ],
            700.0,
        );
        chunks.extend(table_row(
            [
                "ABCDE12345",
                "2024-01-01 08:00:00",
                "First",
                "Completed",
                "100.00",
                "0.00",
                "100.00",
            ],
            680.0,
        ));
        chunks.extend(table_row(
            [
                "FGHIJ67890",
                "2024-01-02 09:00:00",
                "Second",
                "Completed",
                "0.00",
                "50.00",
                "50.00",
            ],
            640.0,
        ));
        // Wrapped details line belonging to the second row.
        chunks.push(chunk("continued", 177.0, 630.0));

        let txns = parse_transactions(&chunks).unwrap();

        assert_eq!(txns.len(), 2);
        assert_eq!(txns[0].receipt_no(), "ABCDE12345");
        assert_eq!(txns[1].receipt_no(), "FGHIJ67890");
        assert_eq!(txns[1].details(), "Second continued");
    }

    #[test]
    fn parse_transactions_without_rows_is_empty() {
        assert!(parse_transactions(&[]).unwrap().is_empty());
    }

    #[test]
    fn short_row_is_an_error() {
        assert!(Transaction::new(&row(&["only one cell"])).is_err());
    }

    #[test]
    fn wrapped_line_at_max_row_height_belongs_to_its_row() {
        let mut chunks = table_row(
            [
                "ABCDE12345",
                "2024-01-01 08:00:00",
                "First",
                "Completed",
                "100.00",
                "0.00",
                "100.00",
            ],
            700.0,
        );
        chunks.push(chunk("continued", 177.0, 630.0));
        chunks.push(chunk("too far", 177.0, 629.0));

        let txns = parse_transactions(&chunks).unwrap();

        assert_eq!(txns.len(), 1);
        assert_eq!(txns[0].details(), "First continued");
    }

    #[test]
    fn owner_is_the_closest_anchor_within_the_height_limit() {
        let anchor = chunk("ABCDE12345", 38.0, 700.0);
        let anchors = [&anchor];

        assert_eq!(owner_index(&anchors, &chunk("x", 177.0, 630.0)), Some(0));
        assert_eq!(owner_index(&anchors, &chunk("x", 177.0, 702.0)), Some(0));
        assert_eq!(owner_index(&anchors, &chunk("x", 177.0, 629.0)), None);
        assert_eq!(owner_index(&anchors, &chunk("x", 177.0, 702.1)), None);
        assert_eq!(owner_index(&[], &chunk("x", 177.0, 700.0)), None);
    }

    #[test]
    fn equal_height_anchors_keep_the_first_row() {
        let first = chunk("ABCDE12345", 38.0, 600.0);
        let second = chunk("FGHIJ67890", 38.0, 600.0);
        let anchors = [&first, &second];

        assert_eq!(owner_index(&anchors, &chunk("x", 177.0, 600.0)), Some(0));
    }
}
