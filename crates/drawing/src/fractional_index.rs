//! Fractional indices: the order keys Excalidraw gives its Elements in
//! `index`, so that an Element can be put between two others without
//! touching them. A port of `@excalidraw/fractional-indexing` (a vendored copy
//! of rocicorp's `fractional-indexing`, CC0) at the pinned commit, base 62.
//!
//! A key is an integer part, whose first letter gives its length (`a` for 2
//! characters, `b` for 3, … and `Z` for 2, `Y` for 3, … below), followed by
//! a fraction. Keys compare as plain strings.

/// A key that is not an order key, or two keys that are not in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Invalid;

const DIGITS: &[u8; 62] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
const ZERO: u8 = DIGITS[0];
const LAST: u8 = DIGITS[61];
/// The smallest integer part, which has no key below it.
const SMALLEST: &str = "A00000000000000000000000000";

fn digit(c: u8) -> Result<usize, Invalid> {
    DIGITS.iter().position(|&d| d == c).ok_or(Invalid)
}

/// The length of the integer part of a key that starts with `head`.
fn integer_length(head: u8) -> Result<usize, Invalid> {
    match head {
        b'a'..=b'z' => Ok(usize::from(head - b'a') + 2),
        b'A'..=b'Z' => Ok(usize::from(b'Z' - head) + 2),
        _ => Err(Invalid),
    }
}

fn integer_part(key: &str) -> Result<&str, Invalid> {
    let length = integer_length(*key.as_bytes().first().ok_or(Invalid)?)?;
    key.get(..length).ok_or(Invalid)
}

/// Whether `key` is an order key.
pub(crate) fn validate(key: &str) -> Result<(), Invalid> {
    if key == SMALLEST || !key.bytes().all(|c| DIGITS.contains(&c)) {
        return Err(Invalid);
    }
    let integer = integer_part(key)?;
    if key[integer.len()..].bytes().last() == Some(ZERO) {
        return Err(Invalid);
    }
    Ok(())
}

/// A fraction between `a` and `b` (`None` is the end), both without a
/// trailing zero, with `a` below `b`.
fn midpoint(a: &[u8], b: Option<&[u8]>) -> Result<Vec<u8>, Invalid> {
    let b = b.filter(|b| !b.is_empty());
    if b.is_some_and(|b| a >= b)
        || a.last() == Some(&ZERO)
        || b.is_some_and(|b| b.last() == Some(&ZERO))
    {
        return Err(Invalid);
    }
    if let Some(b) = b {
        // Take off the common prefix, padding `a` with zeros as it ends.
        let mut common = 0;
        while b.get(common) == Some(&a.get(common).copied().unwrap_or(ZERO)) {
            common += 1;
        }
        if common > 0 {
            let mut key = b[..common].to_vec();
            key.extend(midpoint(
                a.get(common..).unwrap_or_default(),
                Some(&b[common..]),
            )?);
            return Ok(key);
        }
    }
    // The first digits (or lack of one) differ.
    let digit_a = a.first().map_or(Ok(0), |&c| digit(c))?;
    let digit_b = b.map_or(Ok(DIGITS.len()), |b| digit(b[0]))?;
    if digit_b - digit_a > 1 {
        return Ok(vec![DIGITS[(digit_a + digit_b).div_ceil(2)]]);
    }
    // Consecutive digits.
    if let Some(b) = b
        && b.len() > 1
    {
        return Ok(b[..1].to_vec());
    }
    // `b` is a single digit or the end, and `a` starts with the digit below.
    let mut key = vec![DIGITS[digit_a]];
    key.extend(midpoint(a.get(1..).unwrap_or_default(), None)?);
    Ok(key)
}

fn validate_integer(integer: &str) -> Result<(), Invalid> {
    if integer.len() == integer_length(integer.as_bytes()[0])? {
        Ok(())
    } else {
        Err(Invalid)
    }
}

/// The integer after `integer`; `None` after the largest one.
fn increment(integer: &str) -> Result<Option<String>, Invalid> {
    validate_integer(integer)?;
    let mut digits = integer.as_bytes()[1..].to_vec();
    let head = integer.as_bytes()[0];
    let mut carry = true;
    for d in digits.iter_mut().rev() {
        let next = digit(*d)? + 1;
        if next == DIGITS.len() {
            *d = ZERO;
        } else {
            *d = DIGITS[next];
            carry = false;
            break;
        }
    }
    if !carry {
        return Ok(Some(join(head, &digits)));
    }
    if head == b'Z' {
        return Ok(Some("a0".to_owned()));
    }
    if head == b'z' {
        return Ok(None);
    }
    let head = head + 1;
    if head > b'a' {
        digits.push(ZERO);
    } else {
        digits.pop();
    }
    Ok(Some(join(head, &digits)))
}

/// The integer before `integer`; `None` before the smallest one.
fn decrement(integer: &str) -> Result<Option<String>, Invalid> {
    validate_integer(integer)?;
    let mut digits = integer.as_bytes()[1..].to_vec();
    let head = integer.as_bytes()[0];
    let mut borrow = true;
    for d in digits.iter_mut().rev() {
        match digit(*d)? {
            0 => *d = LAST,
            below => {
                *d = DIGITS[below - 1];
                borrow = false;
                break;
            }
        }
    }
    if !borrow {
        return Ok(Some(join(head, &digits)));
    }
    if head == b'a' {
        return Ok(Some("Zz".to_owned()));
    }
    if head == b'A' {
        return Ok(None);
    }
    let head = head - 1;
    if head < b'Z' {
        digits.push(LAST);
    } else {
        digits.pop();
    }
    Ok(Some(join(head, &digits)))
}

fn join(head: u8, digits: &[u8]) -> String {
    let mut key = String::with_capacity(digits.len() + 1);
    key.push(char::from(head));
    key.extend(digits.iter().map(|&d| char::from(d)));
    key
}

/// A key between `a` and `b`, where `None` is the start (for `a`) or the
/// end (for `b`) and `a` is below `b`.
pub(crate) fn key_between(a: Option<&str>, b: Option<&str>) -> Result<String, Invalid> {
    a.map(validate).transpose()?;
    b.map(validate).transpose()?;
    if a.zip(b).is_some_and(|(a, b)| a >= b) {
        return Err(Invalid);
    }
    let (a, b) = match (a, b) {
        (None, None) => return Ok("a0".to_owned()),
        (None, Some(b)) => {
            let integer = integer_part(b)?;
            let fraction = &b[integer.len()..];
            if integer == SMALLEST {
                let tail = midpoint(b"", Some(fraction.as_bytes()))?;
                return Ok(format!("{integer}{}", String::from_utf8_lossy(&tail)));
            }
            if integer < b {
                return Ok(integer.to_owned());
            }
            return decrement(integer)?.ok_or(Invalid);
        }
        (Some(a), None) => {
            let integer = integer_part(a)?;
            let fraction = &a[integer.len()..];
            return match increment(integer)? {
                Some(next) => Ok(next),
                None => {
                    let tail = midpoint(fraction.as_bytes(), None)?;
                    Ok(format!("{integer}{}", String::from_utf8_lossy(&tail)))
                }
            };
        }
        (Some(a), Some(b)) => (a, b),
    };
    let (integer_a, integer_b) = (integer_part(a)?, integer_part(b)?);
    let (fraction_a, fraction_b) = (&a[integer_a.len()..], &b[integer_b.len()..]);
    let between = |tail: Vec<u8>| format!("{integer_a}{}", String::from_utf8_lossy(&tail));
    if integer_a == integer_b {
        return Ok(between(midpoint(
            fraction_a.as_bytes(),
            Some(fraction_b.as_bytes()),
        )?));
    }
    let next = increment(integer_a)?.ok_or(Invalid)?;
    if next.as_str() < b {
        return Ok(next);
    }
    Ok(between(midpoint(fraction_a.as_bytes(), None)?))
}

/// `n` distinct keys in order between `a` and `b`; short ones if both are
/// given.
pub(crate) fn keys_between(
    a: Option<&str>,
    b: Option<&str>,
    n: usize,
) -> Result<Vec<String>, Invalid> {
    match (n, a, b) {
        (0, ..) => Ok(Vec::new()),
        (1, ..) => Ok(vec![key_between(a, b)?]),
        (_, _, None) => {
            let mut keys = vec![key_between(a, None)?];
            for _ in 1..n {
                let next = key_between(keys.last().map(String::as_str), None)?;
                keys.push(next);
            }
            Ok(keys)
        }
        (_, None, Some(_)) => {
            let mut keys = vec![key_between(None, b)?];
            for _ in 1..n {
                let previous = key_between(None, keys.last().map(String::as_str))?;
                keys.push(previous);
            }
            keys.reverse();
            Ok(keys)
        }
        (_, Some(_), Some(_)) => {
            let middle = n / 2;
            let c = key_between(a, b)?;
            let mut keys = keys_between(a, Some(&c), middle)?;
            let above = keys_between(Some(&c), b, n - middle - 1)?;
            keys.push(c);
            keys.extend(above);
            Ok(keys)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Worked out with the vendored TypeScript implementation.
    #[test]
    fn a_key_between_two_keys_matches_excalidraws() {
        let cases = [
            (None, None, "a0"),
            (None, Some("a0"), "Zz"),
            (Some("a0"), None, "a1"),
            (Some("a0"), Some("a1"), "a0V"),
            (Some("a0V"), Some("a1"), "a0l"),
            (Some("Zz"), Some("a0"), "ZzV"),
            (Some("Zz"), Some("a1"), "a0"),
            (None, Some("Y00"), "Xzzz"),
            (Some("bzz"), None, "c000"),
            (Some("a0"), Some("a0V"), "a0G"),
            (Some("a0"), Some("a0G"), "a08"),
            (Some("b125"), Some("b129"), "b127"),
            (Some("a0"), Some("a1V"), "a1"),
            (Some("Zz"), Some("a01"), "a0"),
            (None, Some("a0V"), "a0"),
            (None, Some("b999"), "b99"),
        ];
        for (a, b, expected) in cases {
            assert_eq!(key_between(a, b).as_deref(), Ok(expected), "{a:?} {b:?}");
        }
    }

    #[test]
    fn many_keys_between_two_keys_match_excalidraws() {
        let keys = |a, b, n| keys_between(a, b, n).unwrap();
        assert_eq!(keys(None, None, 5), ["a0", "a1", "a2", "a3", "a4"]);
        assert_eq!(
            keys(Some("a4"), None, 10),
            ["a5", "a6", "a7", "a8", "a9", "aA", "aB", "aC", "aD", "aE"]
        );
        assert_eq!(keys(None, Some("a0"), 5), ["Zv", "Zw", "Zx", "Zy", "Zz"]);
        assert_eq!(
            keys(Some("a0"), Some("a2"), 20),
            [
                "a04", "a08", "a0G", "a0K", "a0O", "a0V", "a0Z", "a0d", "a0l", "a0t", "a1", "a14",
                "a18", "a1G", "a1O", "a1V", "a1Z", "a1d", "a1l", "a1t"
            ]
        );
        assert_eq!(keys(Some("a0"), Some("a1"), 3), ["a0G", "a0V", "a0l"]);
        assert!(keys(Some("a0"), Some("a1"), 0).is_empty());
    }

    #[test]
    fn keys_that_are_not_in_order_are_refused() {
        assert_eq!(key_between(Some("a1"), Some("a0")), Err(Invalid));
        assert_eq!(key_between(Some("a1"), Some("a1")), Err(Invalid));
    }

    #[test]
    fn only_order_keys_are_valid() {
        for key in ["a0", "Zz", "a0V", "b127", "A00000000000000000000000001"] {
            assert_eq!(validate(key), Ok(()), "{key}");
        }
        // Empty, a bad head, too short for its head, a trailing zero, a
        // character outside the digits, and the smallest integer.
        for key in ["", "0", "b1", "a10", "a-", SMALLEST] {
            assert_eq!(validate(key), Err(Invalid), "{key}");
        }
    }
}
