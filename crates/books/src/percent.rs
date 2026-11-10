//! Percent-decoding, as EPUB hrefs use it.

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
            .and_then(|digits| Some(hex_digit(digits[0])? << 4 | hex_digit(digits[1])?));
        if let Some(byte) = escaped {
            out.push(byte);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_digit(digit: u8) -> Option<u8> {
    Some(char::from(digit).to_digit(16)? as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_become_their_bytes() {
        assert_eq!(decode("a%20b%2Fc%c3%a9"), "a b/cé");
        assert_eq!(decode("plain"), "plain");
    }

    #[test]
    fn invalid_escapes_stay() {
        assert_eq!(decode("100%"), "100%");
        assert_eq!(decode("%zz%4"), "%zz%4");
        assert_eq!(decode("%+f"), "%+f");
    }

    #[test]
    fn invalid_utf8_is_replaced() {
        assert_eq!(decode("%ff"), "\u{fffd}");
    }
}
