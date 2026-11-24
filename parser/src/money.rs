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

/// A fixed-size `fmt::Write` target, so formatting a `Decimal` does not
/// allocate. Writes that would not fit fail instead of truncating.
struct StackBuf {
    buf: [u8; 48],
    len: usize,
}

impl StackBuf {
    fn new() -> Self {
        Self {
            buf: [0; 48],
            len: 0,
        }
    }

    fn as_str(&self) -> &str {
        std::str::from_utf8(&self.buf[..self.len]).expect("only str data is written")
    }
}

impl fmt::Write for StackBuf {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let end = self.len + s.len();
        self.buf
            .get_mut(self.len..end)
            .ok_or(fmt::Error)?
            .copy_from_slice(s.as_bytes());
        self.len = end;
        Ok(())
    }
}

impl fmt::Display for Money {
    /// Formats the amount with thousands separators and two decimal places
    /// or as an empty string if the amount is zero.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use fmt::Write as _;

        let dec = self.0;
        if dec == Decimal::ZERO {
            return Ok(());
        }

        // A Decimal has at most 29 digits, which always fits the stack buffer;
        // the heap fallback only keeps this correct if that ever changes.
        let mut stack = StackBuf::new();
        let heap;
        let s = if write!(stack, "{:.2}", dec.abs()).is_ok() {
            stack.as_str()
        } else {
            heap = format!("{:.2}", dec.abs());
            heap.as_str()
        };
        let Some((int_part, frac_part)) = s.split_once('.') else {
            return f.write_str(s);
        };

        // Assemble the whole amount (at most 29 digits, 9 separators, a sign, a
        // point and 2 decimals: 42 bytes), then hand it over in one write.
        let digits = int_part.as_bytes();
        let mut out = StackBuf::new();
        if dec.is_sign_negative() {
            out.write_char('-')?;
        }
        for (i, &d) in digits.iter().enumerate() {
            if i > 0 && (digits.len() - i).is_multiple_of(3) {
                out.write_char(',')?;
            }
            out.write_char(char::from(d))?;
        }
        out.write_char('.')?;
        out.write_str(frac_part)?;
        f.write_str(out.as_str())
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

    #[test]
    fn groups_digits_at_every_boundary() {
        for (input, shown) in [
            ("0.5", "0.50"),
            ("999.99", "999.99"),
            ("1000", "1,000.00"),
            ("12345.67", "12,345.67"),
            ("100000", "100,000.00"),
            ("-1000000", "-1,000,000.00"),
            ("792281625142", "792,281,625,142.00"),
        ] {
            assert_eq!(Money::try_from(input).unwrap().to_string(), shown, "{input}");
        }
    }
}
