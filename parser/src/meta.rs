//! The customer/statement metadata that appears at the top of an M-Pesa statement
//! (customer name, contact details, statement period and request date)

use chrono::NaiveDate;
use pdf_inspector::TextItem;
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::errors::MParserError;

/// The `strftime` format M-Pesa uses for dates in the metadata block, e.g `"01 Jan 2024"`.
const PERIOD_STR: &str = "%d %b %Y";

/// The date range a statement covers, parsed from a `"<from> - <to>"`
/// string such as `"01 Jan 2024 - 31 Jan 2024"`.
#[derive(Debug, Deserialize, Serialize)]
pub struct StatementPeriod {
    /// The first day covered by the statement.
    from_date: NaiveDate,
    /// The last day covered by the statement.
    to_date: NaiveDate,
}

impl fmt::Display for StatementPeriod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} - {}",
            self.from_date.format(PERIOD_STR),
            self.to_date.format(PERIOD_STR)
        )
    }
}

impl TryFrom<&str> for StatementPeriod {
    type Error = MParserError;

    /// Parses a `"<from date> - <to date>"` string, where each date uses the
    /// [`PERIOD_STR`] format (e.g. `"01 Jan 2024 - 31 Jan 2024"`).
    fn try_from(period: &str) -> Result<Self, Self::Error> {
        match period.split_once(" - ") {
            Some((from_date, to_date)) => Ok(Self {
                from_date: NaiveDate::parse_from_str(from_date, PERIOD_STR)?,
                to_date: NaiveDate::parse_from_str(to_date, PERIOD_STR)?,
            }),
            None => Err(MParserError::InvalidStatementPeriod(period.to_owned())),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RequestDate(NaiveDate);

impl fmt::Display for RequestDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.format(PERIOD_STR),)
    }
}

impl TryFrom<&str> for RequestDate {
    type Error = MParserError;

    /// Parses a `"<request date>"` string, where each date uses the
    /// [`PERIOD_STR`] format (e.g. `"01 Jan 2024"`).
    fn try_from(request_date: &str) -> Result<Self, Self::Error> {
        Ok(Self(NaiveDate::parse_from_str(request_date, PERIOD_STR)?))
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

impl Meta {
    /// Reads the metadata block from the next 10 text items yielded by `statement_text_items_iter`
    /// iterator (label/value pairs), consuming them in the process.
    ///
    /// # Errors
    ///
    /// Returns [`MParserError::StatementMetaParseError(field)`] if parsing fails
    pub fn new<T: Iterator<Item = TextItem>>(
        statement_text_items_iter: &mut T,
    ) -> Result<Self, MParserError> {
        use MParserError::StatementMeta;

        let mut iter = statement_text_items_iter
            .take(10)
            .skip(1) // Skip the field label
            .step_by(2);

        Ok(Self {
            customer_name: iter
                .next()
                .ok_or(StatementMeta("customer name".to_string()))?
                .text,
            customer_mobile_number: iter
                .next()
                .ok_or(StatementMeta("customer mobile number".to_string()))?
                .text,
            customer_email_address: iter
                .next()
                .ok_or(StatementMeta("customer email address".to_string()))?
                .text,
            statement_period: iter
                .next()
                .ok_or(StatementMeta("statement period".to_string()))?
                .text
                .as_str()
                .try_into()?,
            statement_request_date: iter
                .next()
                .ok_or(StatementMeta("request date".to_string()))?
                .text
                .as_str()
                .try_into()?,
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
    use pdf_inspector::types::ItemType;

    /// Helper function to build a minimal [`TextItem`] with the given text
    fn text_item(text: &str) -> TextItem {
        TextItem {
            text: text.to_owned(),
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
            font: String::new(),
            font_size: 0.0,
            page: 1,
            is_bold: false,
            is_italic: false,
            is_underline: false,
            is_strikeout: false,
            item_type: ItemType::Text,
            mcid: None,
            font_tag: String::new(),
            legacy_symbol_rewrite: false,
            font_weight: None,
            bold_source: None,
            fixed_pitch: None,
            fill_color: None,
            stroke_color: None,
            render_mode: None,
            rotation: 0.0,
            advance_known: true,
            baseline_shift: 0.0,
        }
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
        let items = vec![
            text_item("Customer Name:"),
            text_item("Jane Doe"),
            text_item("Mobile Number:"),
            text_item("0712345678"),
            text_item("Email Address:"),
            text_item("jane@example.com"),
            text_item("Statement Period:"),
            text_item("01 Jan 2024 - 31 Jan 2024"),
            text_item("Request Date:"),
            text_item("01 Feb 2024"),
        ];
        let mut statement_text_items_iter = items.into_iter();

        let meta = Meta::new(&mut statement_text_items_iter).unwrap();

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
    fn invalid_metadata_block_does_not_parse() {
        let items = vec![
            text_item("Customer Name:"),
            text_item("Jane Doe"),
            text_item("Mobile No:"),
            text_item("0712345678"),
        ];
        let mut iter = items.into_iter();

        assert!(Meta::new(&mut iter).is_err());
    }
}
