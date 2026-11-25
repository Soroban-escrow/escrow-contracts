//! Shared checked arithmetic for escrow balance and fee accounting.
//!
//! Return a typed error on overflow instead of panicking or wrapping.

use crate::errors::Error;

pub(crate) fn add(a: i128, b: i128) -> Result<i128, Error> {
    a.checked_add(b).ok_or(Error::Overflow)
}

pub(crate) fn sub(a: i128, b: i128) -> Result<i128, Error> {
    a.checked_sub(b).ok_or(Error::Overflow)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_arithmetic_preserves_results_and_overflow_errors() {
        assert_eq!(add(2, 3), Ok(5));
        assert_eq!(sub(5, 3), Ok(2));
        assert_eq!(add(i128::MAX, 1), Err(Error::Overflow));
        assert_eq!(add(i128::MIN, -1), Err(Error::Overflow));
        assert_eq!(sub(i128::MIN, 1), Err(Error::Overflow));
        assert_eq!(sub(i128::MAX, -1), Err(Error::Overflow));
    }
}
