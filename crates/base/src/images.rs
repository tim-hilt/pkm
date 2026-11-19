//! Image formats shared by the Contexts that keep Images beside their text
//! (ADR-0029): PNG, JPEG, GIF and WebP, named `<id>.<ext>`.

use crate::item_id::ItemId;

/// The extension of a PNG, JPEG, GIF or WebP by its first bytes; none for
/// anything else, which the client converts to PNG first.
pub fn extension(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("png")
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        Some("jpg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("gif")
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("webp")
    } else {
        None
    }
}

/// Whether `bytes` are a PNG, JPEG, GIF or WebP.
pub fn is_image(bytes: &[u8]) -> bool {
    extension(bytes).is_some()
}

/// Whether a file name is an Image's: `<id>.<ext>` with a kept format's extension.
pub fn is_image_file(name: &str) -> bool {
    name.split_once('.').is_some_and(|(id, extension)| {
        ItemId::parse(id).is_some() && matches!(extension, "png" | "jpg" | "gif" | "webp")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_are_told_by_their_first_bytes() {
        assert_eq!(extension(b"\x89PNG\r\n\x1a\n"), Some("png"));
        assert_eq!(extension(b"\xff\xd8\xff\xe0"), Some("jpg"));
        assert_eq!(extension(b"GIF89a"), Some("gif"));
        assert_eq!(extension(b"RIFF\x01\x00\x00\x00WEBPVP8 "), Some("webp"));
        assert_eq!(extension(b"II*\0 rest"), None);
        assert!(is_image(b"GIF87a") && !is_image(b""));
    }

    #[test]
    fn only_an_id_with_a_kept_extension_names_an_image() {
        assert!(is_image_file("aaaaaaaaaaaa.png"));
        for name in ["aaaaaaaaaaaa.md", "../a.png", "x.png", "AAAAAAAAAAAA.png"] {
            assert!(!is_image_file(name), "{name}");
        }
    }
}
