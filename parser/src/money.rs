use crate::errors::MParserError;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::fmt;

/// A monetary amount from a statement
///
/// Displaying a `Money` renders it with thousands separators and two
/// decimal places (e.g. `"1,234.50"`) or as an empty string when the
/// amount is zero.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Money(Decimal);

impl Money {
    /// A zero amount.
    pub fn zero() -> Self {
        Self(Decimal::ZERO)
    }

    /// The underlying decimal amount.
    pub fn amount(&self) -> Decimal {
        self.0
    }
}

impl std::ops::AddAssign<&Money> for Money {
    fn add_assign(&mut self, rhs: &Money) {
        self.0 += rhs.0;
    }
}

impl TryFrom<&str> for Money {
    type Error = MParserError;

    /// Parses a monetary amount as it appears in a statement table cell,
    /// e.g. `"1,234.50"`. An empty string is treated as zero.
    fn try_from(amount: &str) -> Result<Self, Self::Error> {
        if amount.is_empty() {
            return Ok(Self(Decimal::ZERO));
        }
        let amount = Decimal::from_str_exact(&amount.replace(",", ""))?;
        Ok(Self(amount))
    }
}

impl fmt::Display for Money {
    /// Formats the amount with thousands separators and two decimal places
    /// or as an empty string if the amount is zero.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let dec = self.0;
        if dec == Decimal::ZERO {
            return write!(f, "");
        }
        let s = format!("{:.2}", dec.abs());
        let Some((int_part, frac_part)) = s.split_once('.') else {
            return write!(f, "{s}");
        };

        let mut int_part: String = int_part
            .chars()
            .rev()
            .collect::<Vec<char>>()
            .chunks(3)
            .map(|c| c.iter().rev().collect::<String>())
            .rev()
            .collect::<Vec<String>>()
            .join(",");

        if dec.is_sign_negative() {
            int_part = format!("-{int_part}");
        }

        write!(f, "{int_part}.{frac_part}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_string_parses_as_zero() {
        assert_eq!(Money::try_from("").unwrap().0, Decimal::ZERO);
    }

    #[test]
    fn parses_amounts_with_thousands_separators() {
        assert_eq!(
            Money::try_from("1,234.50").unwrap().0,
            Decimal::new(123450, 2)
        );
    }

    #[test]
    fn rejects_invalid_amounts() {
        assert!(Money::try_from("not a number").is_err());
    }

    #[test]
    fn zero_displays_as_empty_string() {
        let money = Money(Decimal::ZERO);
        assert_eq!(money.to_string(), "");
    }

    #[test]
    fn displays_with_thousands_separators() {
        let money = Money::try_from("1234567.8").unwrap();
        assert_eq!(money.to_string(), "1,234,567.80");
    }

    #[test]
    fn displays_amounts_under_a_thousand_without_separators() {
        let money = Money::try_from("42.5").unwrap();
        assert_eq!(money.to_string(), "42.50");
    }

    #[test]
    fn displays_negative_amounts() {
        let money = Money::try_from("-1234.5").unwrap();
        assert_eq!(money.to_string(), "-1,234.50");
    }
}
