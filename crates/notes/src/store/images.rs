//! Images (ADR-0029, ADR-0030): `<id>.<ext>` in the directory of the Notes
//! that show them, referenced from a Body as `![](<id>.<ext>)`. A reference
//! means the Image beside the Note, so only Notes in one Folder share one.

use super::Notes;
use pkm_base::images::{extension, is_image_file};
use pkm_base::item_id::ItemId;
use pkm_base::markdown::image_targets;
use pkm_base::random::Entropy;
use std::collections::HashSet;
use std::io;
use std::path::Path;

impl Notes {
    /// Keeps `bytes`, a PNG, JPEG, GIF or WebP, as they are beside the Note
    /// `id`; the reference to write into its Body. `InvalidData` for any other
    /// format, `NotFound` if there is no such Note.
    pub fn add_image(&self, id: &str, bytes: &[u8]) -> io::Result<String> {
        let extension = extension(bytes).ok_or(io::ErrorKind::InvalidData)?;
        let (_, path, _) = self.find(id)?;
        let reference = format!("{}.{extension}", ItemId::new(&mut Entropy::from_system()));
        self.files.write_bytes(&beside(&path, &reference), bytes)?;
        Ok(reference)
    }

    /// The bytes of the Image `reference` names beside the Note `id`;
    /// `NotFound` if there is none, or if `reference` isn't one (so it can't
    /// reach other files).
    pub fn read_image(&self, id: &str, reference: &str) -> io::Result<Vec<u8>> {
        let (_, path, _) = self.find(id)?;
        if !is_image_file(reference) {
            return Err(io::ErrorKind::NotFound.into());
        }
        self.files.read_bytes(&beside(&path, reference))
    }

    /// Deletes those of the Images `references` beside the Note `id` that no
    /// Note there shows any more; others, and anything that isn't an Image,
    /// stay. For a client leaving the Note, with the Images it showed or added
    /// meanwhile (never a sweep of every unreferenced Image, since an Image
    /// can Sync before the Note that shows it).
    pub fn release_images(&self, id: &str, references: &[String]) -> io::Result<()> {
        if references.is_empty() {
            return Ok(()); // without reading every Note
        }
        let (_, path, _) = self.find(id)?;
        self.release_beside(&path, references)
    }

    /// Releases `references` beside the file at `path` (see `release_images`).
    pub(super) fn release_beside(&self, path: &Path, references: &[String]) -> io::Result<()> {
        let dir = path.parent().unwrap_or(&self.root);
        let shown = self.shown_in(dir, None)?;
        for reference in references.iter().filter(|r| !shown.contains(*r)) {
            if !is_image_file(reference) {
                continue;
            }
            match self.files.remove(&dir.join(reference)) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
                _ => {}
            }
        }
        Ok(())
    }

    /// Moves the Images of the Note `id` (its Body is `body`) from beside
    /// `from` to beside `to`, except those another Note beside `from` shows.
    pub(super) fn move_images(
        &self,
        id: &ItemId,
        body: &str,
        from: &Path,
        to: &Path,
    ) -> io::Result<()> {
        let others = self.shown_in(from.parent().unwrap_or(&self.root), Some(id))?;
        for reference in image_targets(body)
            .iter()
            .filter(|r| is_image_file(r) && !others.contains(*r))
        {
            match self
                .files
                .rename(&beside(from, reference), &beside(to, reference))
            {
                Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
                _ => {}
            }
        }
        Ok(())
    }

    /// The references the Notes directly in `dir` show, except those of `skip`.
    fn shown_in(&self, dir: &Path, skip: Option<&ItemId>) -> io::Result<HashSet<String>> {
        Ok(self
            .notes_in(dir)?
            .iter()
            .filter(|(id, _)| Some(id) != skip)
            .flat_map(|(_, file)| image_targets(&file.body))
            .collect())
    }
}

/// The path of the file `name` in the directory of the Note file at `note`.
fn beside(note: &Path, name: &str) -> std::path::PathBuf {
    note.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::super::tests::Scratch;
    use super::*;

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n rest";

    fn show(notes: &Notes, id: &str, images: &[&str]) {
        let body: String = images.iter().map(|r| format!("![]({r})\n\n")).collect();
        notes.edit(id, "t", &body).unwrap();
    }

    #[test]
    fn an_added_image_is_kept_byte_for_byte_beside_its_note_and_read_back() {
        let scratch = Scratch::new();
        let notes = scratch.store();
        notes.create_folder("", "Work").unwrap();
        let id = notes.create("Work").unwrap().id;
        let reference = notes.add_image(&id, PNG).unwrap();
        assert!(reference.ends_with(".png") && ItemId::parse(&reference[..12]).is_some());
        assert_eq!(fs_read(&scratch, &format!("Work/{reference}")), PNG);
        assert_eq!(notes.read_image(&id, &reference).unwrap(), PNG);
        assert_ne!(notes.add_image(&id, PNG).unwrap(), reference);
    }

    #[test]
    fn other_formats_are_refused_and_only_images_can_be_read() {
        let scratch = Scratch::new();
        let notes = scratch.store();
        let id = notes.create("").unwrap().id;
        let error = notes.add_image(&id, b"II*\0 rest").unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert_eq!(
            notes.add_image("aaaaaaaaaaaa", PNG).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        for reference in ["missing.png", "../a.png", &format!("{id}.md")] {
            let error = notes.read_image(&id, reference).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::NotFound, "{reference}");
        }
    }

    #[test]
    fn release_deletes_only_unshown_images_of_the_given() {
        let scratch = Scratch::new();
        let notes = scratch.store();
        let id = notes.create("").unwrap().id;
        let (shown, loose, other) = (
            notes.add_image(&id, PNG).unwrap(),
            notes.add_image(&id, PNG).unwrap(),
            notes.add_image(&id, PNG).unwrap(),
        );
        show(&notes, &id, &[&shown]);
        notes
            .release_images(&id, &[shown.clone(), loose.clone(), "../x.md".into()])
            .unwrap();
        assert!(scratch.exists_file(&shown) && scratch.exists_file(&other));
        assert!(!scratch.exists_file(&loose));
        notes.release_images(&id, &[loose]).unwrap();
    }

    #[test]
    fn an_image_two_notes_show_stays_until_both_let_go() {
        let scratch = Scratch::new();
        let notes = scratch.store();
        let (a, b) = (notes.create("").unwrap().id, notes.create("").unwrap().id);
        let image = notes.add_image(&a, PNG).unwrap();
        show(&notes, &a, &[&image]);
        show(&notes, &b, &[&image]);
        show(&notes, &a, &[]);
        notes
            .release_images(&a, std::slice::from_ref(&image))
            .unwrap();
        assert!(scratch.exists_file(&image));
        show(&notes, &b, &[]);
        notes
            .release_images(&b, std::slice::from_ref(&image))
            .unwrap();
        assert!(!scratch.exists_file(&image));
    }

    #[test]
    fn deleting_a_note_deletes_its_images_unless_another_shows_them() {
        let scratch = Scratch::new();
        let notes = scratch.store();
        let (a, b) = (notes.create("").unwrap().id, notes.create("").unwrap().id);
        let (own, shared) = (
            notes.add_image(&a, PNG).unwrap(),
            notes.add_image(&a, PNG).unwrap(),
        );
        show(&notes, &a, &[&own, &shared]);
        show(&notes, &b, &[&shared]);
        notes.delete(&a).unwrap();
        assert!(!scratch.exists_file(&own));
        assert!(scratch.exists_file(&shared));
    }

    #[test]
    fn deleting_a_folder_deletes_its_images() {
        let scratch = Scratch::new();
        let notes = scratch.store();
        notes.create_folder("", "Work").unwrap();
        let id = notes.create("Work").unwrap().id;
        let image = notes.add_image(&id, PNG).unwrap();
        show(&notes, &id, &[&image]);
        notes.delete_folder("Work").unwrap();
        assert!(!scratch.0.join("Work").exists());
    }

    #[test]
    fn moving_a_note_moves_the_images_only_it_shows() {
        let scratch = Scratch::new();
        let notes = scratch.store();
        notes.create_folder("", "Work").unwrap();
        let (a, b) = (notes.create("").unwrap().id, notes.create("").unwrap().id);
        let (own, shared) = (
            notes.add_image(&a, PNG).unwrap(),
            notes.add_image(&a, PNG).unwrap(),
        );
        show(&notes, &a, &[&own, &shared]);
        show(&notes, &b, &[&shared]);
        notes.move_note(&a, "Work").unwrap();
        assert!(scratch.exists_file(&format!("Work/{own}")));
        assert!(!scratch.exists_file(&own));
        assert!(scratch.exists_file(&shared));
        assert!(!scratch.exists_file(&format!("Work/{shared}")));
        assert_eq!(notes.read_image(&a, &own).unwrap(), PNG);
    }

    fn fs_read(scratch: &Scratch, name: &str) -> Vec<u8> {
        std::fs::read(scratch.0.join(name)).unwrap()
    }
}
