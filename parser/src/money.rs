use crate::errors::Error;
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
    /// The underlying decimal amount.
    pub fn amount(&self) -> Decimal {
        self.0
    }
}

/// Capacity of the stack buffer used to strip thousands separators from an
/// amount. Sized for the longest plausible money amount (`-999999999999.99`).
const MAX_AMOUNT_BYTES: usize = 16;

impl TryFrom<&str> for Money {
    type Error = Error;

    /// Parses a monetary amount as it appears in a statement table cell,
    /// e.g. `"1,234.50"`. Thousands separators are ignored and an empty
    /// string is treated as zero.
    ///
    /// # Errors
    ///
    /// Returns an error if the amount is not a valid decimal number.
    fn try_from(amount: &str) -> Result<Self, Self::Error> {
        if amount.is_empty() {
            return Ok(Self(Decimal::ZERO));
        }

        let invalid = || Error::InvalidAmount(amount.to_owned());

        let mut buf = [0u8; MAX_AMOUNT_BYTES];
        let mut len = 0;
        for &b in amount.as_bytes() {
            if b == b',' {
                continue;
            }

            buf[len] = b;
            len += 1;
        }

        let cleaned = std::str::from_utf8(&buf[..len]).map_err(|_| invalid())?;
        Decimal::from_str_exact(cleaned)
            .map(Self)
            .map_err(|_| invalid())
    }
}

impl fmt::Display for Money {
    /// Formats the amount with thousands separators and two decimal places
    /// or as an empty string if the amount is zero.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let dec = self.0;
        if dec == Decimal::ZERO {
            return Ok(());
        }
        let s = format!("{:.2}", dec.abs());
        let Some((int_part, frac_part)) = s.split_once('.') else {
            return f.write_str(&s);
        };

        if dec.is_sign_negative() {
            f.write_str("-")?;
        }
        let digits = int_part.as_bytes();
        for i in 0..digits.len() {
            if i > 0 && (digits.len() - i).is_multiple_of(3) {
                f.write_str(",")?;
            }
            f.write_str(std::str::from_utf8(&digits[i..=i]).expect("digit is ASCII"))?;
        }
        f.write_str(".")?;
        f.write_str(frac_part)
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
