//! Images (ADR-0029): `Tasks/<id>.<ext>` beside the Task files, referenced from
//! a Description as `![](<id>.<ext>)`.

use super::Tasks;
use pkm_base::item_id::ItemId;
use pkm_base::random::Entropy;
use std::io;
use std::path::PathBuf;

/// The extension of a PNG, JPEG, GIF or WebP by its first bytes; none for
/// anything else, which the client converts to PNG first.
fn extension(bytes: &[u8]) -> Option<&'static str> {
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

impl Tasks {
    /// Keeps `bytes`, a PNG, JPEG, GIF or WebP, as they are beside the Tasks;
    /// the reference to write into a Description. Fails with `InvalidData` for
    /// any other format.
    pub fn add_image(&self, bytes: &[u8]) -> io::Result<String> {
        let extension = extension(bytes).ok_or(io::ErrorKind::InvalidData)?;
        let id = ItemId::new(&mut Entropy::from_system());
        let reference = format!("{id}.{extension}");
        self.files.write_bytes(&self.root.join(&reference), bytes)?;
        Ok(reference)
    }

    /// The bytes of the Image `reference` names; `NotFound` if there is none,
    /// or if `reference` isn't one (so it can't reach other files).
    pub fn read_image(&self, reference: &str) -> io::Result<Vec<u8>> {
        let path = self.image_path(reference).ok_or(io::ErrorKind::NotFound)?;
        self.files.read_bytes(&path)
    }

    fn image_path(&self, reference: &str) -> Option<PathBuf> {
        let (id, extension) = reference.split_once('.')?;
        ItemId::parse(id)?;
        matches!(extension, "png" | "jpg" | "gif" | "webp").then(|| self.root.join(reference))
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::Scratch;
    use super::*;

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n rest";
    const JPEG: &[u8] = b"\xff\xd8\xff\xe0 rest";
    const GIF: &[u8] = b"GIF89a rest";
    const WEBP: &[u8] = b"RIFF\x01\x00\x00\x00WEBPVP8 ";

    #[test]
    fn an_added_image_is_kept_byte_for_byte_beside_the_tasks() {
        let scratch = Scratch::new();
        let tasks = scratch.store();
        for (bytes, extension) in [(PNG, "png"), (JPEG, "jpg"), (GIF, "gif"), (WEBP, "webp")] {
            let reference = tasks.add_image(bytes).unwrap();
            assert!(reference.ends_with(&format!(".{extension}")), "{reference}");
            assert!(ItemId::parse(&reference[..12]).is_some());
            assert_eq!(std::fs::read(scratch.0.join(&reference)).unwrap(), bytes);
            assert_eq!(tasks.read_image(&reference).unwrap(), bytes);
        }
    }

    #[test]
    fn two_images_get_two_references() {
        let tasks = Scratch::new().store();
        assert_ne!(tasks.add_image(PNG).unwrap(), tasks.add_image(PNG).unwrap());
    }

    #[test]
    fn other_formats_are_refused() {
        let scratch = Scratch::new();
        let tasks = scratch.store();
        let tiff = tasks.add_image(b"II*\0 rest").unwrap_err();
        assert_eq!(tiff.kind(), io::ErrorKind::InvalidData);
        assert!(tasks.add_image(b"").is_err());
        assert!(tasks.rows().unwrap().is_empty());
    }

    #[test]
    fn only_images_can_be_read() {
        let scratch = Scratch::new();
        let tasks = scratch.store();
        let task = tasks.create("a", 4, &[]).unwrap();
        for reference in [
            "missing.png",
            "aaaaaaaaaaaa.png",
            "../Tags.md",
            &format!("{}.md", task.id),
            &format!("{}.png", task.id.to_uppercase()),
        ] {
            let error = tasks.read_image(reference).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::NotFound, "{reference}");
        }
    }
}
