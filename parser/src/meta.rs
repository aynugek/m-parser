//! The customer/statement metadata that appears at the top of an M-Pesa statement
//! (customer name, contact details, statement period and request date)

use jiff::civil::Date;
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::{
    Error::StatementMeta,
    errors::Error,
    pdf::Chunk,
    statement::{clean_label, find},
};

/// The `strftime` format M-Pesa uses for dates in the metadata block, e.g `"01 Jan 2024"`.
const PERIOD_STR: &str = "%d %b %Y";

/// The date range a statement covers, parsed from a `"<from> - <to>"`
/// string such as `"01 Jan 2024 - 31 Jan 2024"`.
#[derive(Debug, Deserialize, Serialize)]
pub struct StatementPeriod {
    /// The first day covered by the statement.
    from_date: Date,
    /// The last day covered by the statement.
    to_date: Date,
}

impl fmt::Display for StatementPeriod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} - {}",
            self.from_date.strftime(PERIOD_STR),
            self.to_date.strftime(PERIOD_STR)
        )
    }
}

impl TryFrom<&str> for StatementPeriod {
    type Error = Error;

    /// Parses a `"<from date> - <to date>"` string, where each date uses the
    /// [`PERIOD_STR`] format (e.g. `"01 Jan 2024 - 31 Jan 2024"`).
    fn try_from(period: &str) -> Result<Self, Self::Error> {
        match period.split_once(" - ") {
            Some((from_date, to_date)) => Ok(Self {
                from_date: Date::strptime(PERIOD_STR, from_date)?,
                to_date: Date::strptime(PERIOD_STR, to_date)?,
            }),
            None => Err(Error::InvalidStatementPeriod(period.to_owned())),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RequestDate(Date);

impl fmt::Display for RequestDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.strftime(PERIOD_STR),)
    }
}

impl TryFrom<&str> for RequestDate {
    type Error = Error;

    /// Parses a `"<request date>"` string, where each date uses the
    /// [`PERIOD_STR`] format (e.g. `"01 Jan 2024"`).
    fn try_from(request_date: &str) -> Result<Self, Self::Error> {
        Ok(Self(Date::strptime(PERIOD_STR, request_date)?))
    }
}

/// Customer and statement metadata.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct Meta {
    customer_name: String,
    customer_mobile_number: String,
    customer_email_address: String,
    statement_period: StatementPeriod,
    statement_request_date: RequestDate,
}

pub fn parse_meta(chunks: &[Chunk]) -> Result<Meta, Error> {
    Meta::new(chunks)
}

impl Meta {
    /// Reads the metadata block from the next 10 text items yielded by `statement_text_items_iter`
    /// iterator (label/value pairs), consuming them in the process.
    ///
    /// # Errors
    ///
    /// Returns [`Error::StatementMetaParseError(field)`] if parsing fails
    fn new(chunks: &[Chunk]) -> Result<Self, Error> {
        let summary = find(chunks, "SUMMARY").ok_or(Error::InvalidMeta)?;

        // Customer name, mobile number, email, statement period, request date.
        // Labels sit at x ~ 38 above the "SUMMARY" heading.
        // Values sit at x ~ 195 on the same line.
        // The value baseline can be 1-2pt higher than its label's.
        let mut labels: Vec<&Chunk> = chunks
            .iter()
            .filter(|c| c.x < 100.0 && c.y > summary.y && c.text.trim_end().ends_with(':'))
            .collect();
        labels.sort_by(|a, b| b.y.partial_cmp(&a.y).unwrap());

        let fields: std::collections::HashMap<String, String> = labels
            .iter()
            .map(|l| {
                let mut vals: Vec<&Chunk> = chunks
                    .iter()
                    .filter(|c| c.x >= 150.0 && c.x < 400.0 && (c.y - l.y).abs() <= 4.0)
                    .collect();
                vals.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());
                let v = vals
                    .iter()
                    .map(|c| c.text.trim())
                    .collect::<Vec<_>>()
                    .join(" ");
                (clean_label(&l.text), v)
            })
            .collect();

        let get = |label: &str, field: &str| {
            fields
                .get(label)
                .ok_or_else(|| StatementMeta(field.to_string()))
        };

        Ok(Self {
            customer_name: get("Customer Name", "customer name")?.clone(),
            customer_mobile_number: get("Mobile Number", "customer mobile number")?.clone(),
            customer_email_address: get("Email Address", "customer email address")?.clone(),
            statement_period: get("Statement Period", "statement period")?
                .as_str()
                .try_into()?,
            statement_request_date: get("Request Date", "request date")?.as_str().try_into()?,
        })
    }

    /// The customer's full name.
    pub fn customer_name(&self) -> &str {
        &self.customer_name
    }

    /// The customer's mobile number.
    pub fn customer_mobile_number(&self) -> &str {
        &self.customer_mobile_number
    }

    /// The customer's email address.
    pub fn customer_email_address(&self) -> &str {
        &self.customer_email_address
    }

    /// The date range covered by the statement.
    pub fn statement_period(&self) -> &StatementPeriod {
        &self.statement_period
    }

    /// The date the statement was requested.
    pub fn statement_request_date(&self) -> &RequestDate {
        &self.statement_request_date
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper function to build a [`Chunk`] at the given position.
    fn chunk(text: &str, x: f32, y: f32) -> Chunk {
        Chunk {
            x,
            y,
            text: text.to_owned(),
        }
    }

    /// Builds a label/value row; the label sits left, the value in the value column.
    fn row(label: &str, value: &str, y: f32) -> [Chunk; 2] {
        [chunk(label, 50.0, y), chunk(value, 200.0, y)]
    }

    #[test]
    fn valid_statement_period_parses() {
        assert_eq!(
            StatementPeriod::try_from("01 Jan 2024 - 31 Jan 2024")
                .unwrap()
                .to_string(),
            "01 Jan 2024 - 31 Jan 2024"
        );
    }

    #[test]
    fn invalid_statement_period_does_not_parse() {
        assert!(StatementPeriod::try_from("01 Jan 2024").is_err());
    }

    #[test]
    fn valid_request_date_parses() {
        assert_eq!(
            RequestDate::try_from("01 Jan 2024").unwrap().to_string(),
            "01 Jan 2024"
        );
    }

    #[test]
    fn invalid_request_date_does_not_parse() {
        assert!(RequestDate::try_from("abcde").is_err());
    }

    #[test]
    fn valid_metadata_block_parses() {
        let mut chunks = vec![chunk("SUMMARY", 50.0, 500.0)];
        chunks.extend(row("Customer Name:", "Jane Doe", 700.0));
        chunks.extend(row("Mobile Number:", "0712345678", 680.0));
        chunks.extend(row("Email Address:", "jane@example.com", 660.0));
        chunks.extend(row("Statement Period:", "01 Jan 2024 - 31 Jan 2024", 640.0));
        chunks.extend(row("Request Date:", "01 Feb 2024", 620.0));

        let meta = Meta::new(&chunks).unwrap();

        assert_eq!(meta.customer_name(), "Jane Doe");
        assert_eq!(meta.customer_mobile_number(), "0712345678");
        assert_eq!(meta.customer_email_address(), "jane@example.com");
        assert_eq!(
            meta.statement_period().to_string(),
            "01 Jan 2024 - 31 Jan 2024"
        );
        assert_eq!(meta.statement_request_date().to_string(), "01 Feb 2024");
    }

    #[test]
    fn missing_summary_does_not_parse() {
        let mut chunks = Vec::new();
        chunks.extend(row("Customer Name:", "Jane Doe", 700.0));

        assert!(Meta::new(&chunks).is_err());
    }

    #[test]
    fn invalid_metadata_block_does_not_parse() {
        let mut chunks = vec![chunk("SUMMARY", 50.0, 500.0)];
        chunks.extend(row("Customer Name:", "Jane Doe", 700.0));
        chunks.extend(row("Mobile No:", "0712345678", 680.0));

        assert!(Meta::new(&chunks).is_err());
    }
}
