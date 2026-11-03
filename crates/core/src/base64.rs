//! Base64 (RFC 4648, standard alphabet), the encoding of the files in a
//! Drawing. Written out here because the core has no dependencies (ADR-0009).

const DIGITS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// `bytes` as padded base64 text.
pub(crate) fn encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let group = chunk.iter().enumerate().fold(0u32, |group, (i, &byte)| {
            group | u32::from(byte) << (16 - 8 * i)
        });
        // A chunk of n bytes fills n + 1 digits; the rest is padding.
        for i in 0..4 {
            out.push(if i <= chunk.len() {
                DIGITS[(group >> (18 - 6 * i) & 63) as usize] as char
            } else {
                '='
            });
        }
    }
    out
}

/// The bytes `text` stands for; `None` if it is not valid base64. Padding is
/// optional, and line breaks and other whitespace are skipped, as browsers do.
pub(crate) fn decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let (mut bits, mut count) = (0u32, 0u32);
    let mut digits = text
        .bytes()
        .filter(|b| !b.is_ascii_whitespace())
        .collect::<Vec<u8>>();
    for _ in 0..2 {
        if digits.last() == Some(&b'=') {
            digits.pop();
        }
    }
    // One digit more than a whole number of groups carries no complete byte.
    if digits.len() % 4 == 1 {
        return None;
    }
    for digit in digits {
        let value = match digit {
            b'A'..=b'Z' => digit - b'A',
            b'a'..=b'z' => digit - b'a' + 26,
            b'0'..=b'9' => digit - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        };
        bits = bits << 6 | u32::from(value);
        count += 6;
        if count >= 8 {
            count -= 8;
            out.push((bits >> count) as u8);
            bits &= (1 << count) - 1;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::{decode, encode};

    #[test]
    fn encodes_the_rfc_4648_test_vectors_with_padding() {
        for (plain, encoded) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(encode(plain.as_bytes()), encoded, "{plain}");
        }
    }

    #[test]
    fn every_byte_value_survives_a_round_trip() {
        let bytes: Vec<u8> = (0..=255).collect();
        assert_eq!(decode(&encode(&bytes)), Some(bytes));
    }

    #[test]
    fn decodes_the_rfc_4648_test_vectors() {
        for (encoded, plain) in [
            ("", ""),
            ("Zg==", "f"),
            ("Zm8=", "fo"),
            ("Zm9v", "foo"),
            ("Zm9vYg==", "foob"),
            ("Zm9vYmE=", "fooba"),
            ("Zm9vYmFy", "foobar"),
        ] {
            assert_eq!(
                decode(encoded),
                Some(plain.as_bytes().to_vec()),
                "{encoded}"
            );
        }
    }

    #[test]
    fn padding_is_optional_and_whitespace_is_skipped() {
        assert_eq!(decode("Zm9vYg"), Some(b"foob".to_vec()));
        assert_eq!(decode("Zm9v\r\nYmFy"), Some(b"foobar".to_vec()));
    }

    #[test]
    fn the_url_unsafe_digits_and_all_byte_values_work() {
        assert_eq!(decode("+/8="), Some(vec![0xfb, 0xff]));
    }

    #[test]
    fn anything_else_is_rejected() {
        for bad in ["Zm9v!", "Z", "Zm9vY", "Zm=9", "Zm9v-_"] {
            assert_eq!(decode(bad), None, "{bad}");
        }
    }
}
