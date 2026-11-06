//! Percent-encoding and -decoding, as URLs use them.

use std::fmt::Write as _;

/// Percent-encodes a path segment: everything but unreserved characters.
pub(crate) fn encode(part: &str) -> String {
    let mut out = String::with_capacity(part.len());
    for byte in part.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char);
            }
            // Writing to a `String` can't fail.
            _ => _ = write!(out, "%{byte:02X}"),
        }
    }
    out
}

/// `text` with each `%XX` escape replaced by its byte. An invalid escape stays
/// as it is, and invalid UTF-8 in the result becomes U+FFFD.
pub(crate) fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let escaped = (bytes[i] == b'%')
            .then(|| bytes.get(i + 1..i + 3))
            .flatten()
            .and_then(|digits| {
                let digit = |d: u8| char::from(d).to_digit(16);
                Some((digit(digits[0])? * 16 + digit(digits[1])?) as u8)
            });
        match escaped {
            Some(byte) => {
                out.push(byte);
                i += 3;
            }
            None => {
                out.push(bytes[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_percent_encoded() {
        assert_eq!(encode("a b/ü"), "a%20b%2F%C3%BC");
        assert_eq!(decode(&encode("x&y='z'")), "x&y='z'");
    }

    #[test]
    fn escapes_become_their_bytes() {
        assert_eq!(decode("a%20b%2Fc%c3%a9"), "a b/cé");
        assert_eq!(decode("plain"), "plain");
    }

    #[test]
    fn invalid_escapes_stay_as_they_are() {
        assert_eq!(decode("100%"), "100%");
        assert_eq!(decode("%zz%4"), "%zz%4");
        assert_eq!(decode("%+1"), "%+1");
    }
}
