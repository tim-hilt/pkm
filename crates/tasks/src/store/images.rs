//! Images (ADR-0029): `Tasks/<id>.<ext>` beside the Task files, referenced from
//! a Description as `![](<id>.<ext>)`.

use super::Tasks;
use pkm_base::images::{extension, is_image_file};
use pkm_base::item_id::ItemId;
use pkm_base::markdown::image_targets;
use pkm_base::random::Entropy;
use std::collections::HashSet;
use std::io;
use std::path::PathBuf;

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

    /// Deletes those of the Images `references` that no Task's Description
    /// shows any more; others, and anything that isn't an Image, stay. This
    /// is the only way an Image goes besides its Task's Close (never a sweep
    /// of every unreferenced Image).
    pub fn release_images(&self, references: &[String]) -> io::Result<()> {
        if references.is_empty() {
            return Ok(()); // without reading every Task
        }
        let shown: HashSet<String> = self
            .load()?
            .values()
            .flat_map(|node| image_targets(&node.task.description))
            .collect();
        for reference in references.iter().filter(|r| !shown.contains(*r)) {
            let Some(path) = self.image_path(reference) else {
                continue;
            };
            match self.files.remove(&path) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
                _ => {}
            }
        }
        Ok(())
    }

    fn image_path(&self, reference: &str) -> Option<PathBuf> {
        is_image_file(reference).then(|| self.root.join(reference))
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Closing, tests::Scratch};
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

    fn describe(tasks: &Tasks, id: &str, description: &str) {
        tasks.edit(id, "t", description, 4, &[]).unwrap();
    }

    fn exists(scratch: &Scratch, reference: &str) -> bool {
        scratch.0.join(reference).exists()
    }

    #[test]
    fn release_deletes_only_unreferenced_images() {
        let scratch = Scratch::new();
        let tasks = scratch.store();
        let task = tasks.create("a", 4, &[]).unwrap();
        let shown = tasks.add_image(PNG).unwrap();
        let loose = tasks.add_image(PNG).unwrap();
        let other = tasks.add_image(PNG).unwrap();
        describe(&tasks, &task.id, &format!("![]({shown})"));
        assert!(exists(&scratch, &loose), "nothing releases it but release");
        tasks
            .release_images(&[shown.clone(), loose.clone()])
            .unwrap();
        assert!(exists(&scratch, &shown));
        assert!(!exists(&scratch, &loose));
        assert!(exists(&scratch, &other));
        tasks.release_images(&[loose, "../Tags.md".into()]).unwrap();
    }

    #[test]
    fn an_image_shown_by_two_tasks_stays_until_both_let_go() {
        let scratch = Scratch::new();
        let tasks = scratch.store();
        let (a, b) = (
            tasks.create("a", 4, &[]).unwrap(),
            tasks.create("b", 4, &[]).unwrap(),
        );
        let image = tasks.add_image(PNG).unwrap();
        describe(&tasks, &a.id, &format!("![]({image})"));
        describe(&tasks, &b.id, &format!("![]({image})"));
        describe(&tasks, &a.id, "");
        tasks.release_images(std::slice::from_ref(&image)).unwrap();
        assert!(exists(&scratch, &image));
        describe(&tasks, &b.id, "");
        tasks.release_images(std::slice::from_ref(&image)).unwrap();
        assert!(!exists(&scratch, &image));
    }

    #[test]
    fn committing_a_close_deletes_the_images_unless_another_task_shows_them() {
        let scratch = Scratch::new();
        let tasks = scratch.store();
        let task = tasks.create("a", 4, &[]).unwrap();
        let other = tasks.create("b", 4, &[]).unwrap();
        let (own, shared) = (tasks.add_image(PNG).unwrap(), tasks.add_image(PNG).unwrap());
        describe(&tasks, &task.id, &format!("![]({own})\n\n![]({shared})"));
        describe(&tasks, &other.id, &format!("![]({shared})"));
        tasks.close(&task.id, Closing::Done).unwrap();
        assert!(exists(&scratch, &own));
        tasks.commit().unwrap();
        assert!(!exists(&scratch, &own));
        assert!(exists(&scratch, &shared));
    }

    #[test]
    fn reverting_a_close_keeps_the_images() {
        let scratch = Scratch::new();
        let tasks = scratch.store();
        let task = tasks.create("a", 4, &[]).unwrap();
        let image = tasks.add_image(PNG).unwrap();
        describe(&tasks, &task.id, &format!("![]({image})"));
        tasks.close(&task.id, Closing::Rejected).unwrap();
        tasks.revert();
        tasks.commit().unwrap();
        assert!(exists(&scratch, &image));
    }
}
