//! The Bookmarks store: `<id>.md` files with the front matter `id`,
//! `address`, `title`, `site`, `cover`, `excerpt`, `author`, `published` and
//! `saved`, beside their Cover `<id>.<ext>` (Bookmarks ADR-0002). The body is
//! the Article's place, empty until there are Articles.

use crate::address::normalise;
use crate::extract::Details;
use pkm_base::clock::{iso8601, now_ms};
use pkm_base::files::{DiskFiles, Files};
use pkm_base::front_matter::Document;
use pkm_base::images::{extension, is_image_file};
use pkm_base::item_id::ItemId;
use pkm_base::random::Entropy;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;

const ADDRESS: &str = "address";
const TITLE: &str = "title";
const SITE: &str = "site";
const COVER: &str = "cover";
const EXCERPT: &str = "excerpt";
const AUTHOR: &str = "author";
const PUBLISHED: &str = "published";
const SAVED: &str = "saved";

/// A Bookmark and its details. Empty where there is none, except `cover`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bookmark {
    pub id: String,
    /// Normalised: no two Bookmarks share one.
    pub address: String,
    pub title: String,
    pub site: String,
    /// The file name of the Cover beside the Bookmark.
    pub cover: Option<String>,
    pub excerpt: String,
    pub author: String,
    pub published: String,
    /// When it was Saved, as UTC ISO 8601.
    pub saved: String,
}

/// What Saving gave.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Saved {
    pub bookmark: Bookmark,
    /// The Address was Saved before, so this is that Bookmark, unchanged.
    pub existing: bool,
}

/// The Bookmarks in one folder (`Bookmarks` in the app's storage).
#[derive(Debug)]
pub struct Bookmarks {
    files: Arc<dyn Files>,
    root: PathBuf,
}

impl Bookmarks {
    /// The folder's name, in the app's storage and in the cloud.
    pub const FOLDER: &'static str = "Bookmarks";

    /// Opens the Bookmarks in `root`, creating the folder if needed.
    pub fn open(root: impl Into<PathBuf>) -> io::Result<Self> {
        let root = root.into();
        fs::create_dir_all(&root)?;
        Ok(Self::open_in(Arc::new(DiskFiles), root))
    }

    /// The Bookmarks in `root` on any `files`.
    pub fn open_in(files: Arc<dyn Files>, root: impl Into<PathBuf>) -> Self {
        Self {
            files,
            root: root.into(),
        }
    }

    /// Saves the page at `address` with its `details` (see `extract`) unless
    /// its Address is Saved already, which gives that Bookmark. `title` is the
    /// user's, which a blank one leaves to the page's. `cover` holds the bytes
    /// of the Cover; they are left out if they are no PNG, JPEG, GIF or
    /// WebP, so that a broken image never costs the Bookmark. `InvalidInput`
    /// for an `address` that is none (see `normalise`).
    pub fn save(
        &self,
        address: &str,
        details: &Details,
        title: &str,
        cover: Option<&[u8]>,
    ) -> io::Result<Saved> {
        let address = normalise(address).ok_or(io::ErrorKind::InvalidInput)?;
        if let Some(bookmark) = self.list()?.into_iter().find(|b| b.address == address) {
            return Ok(Saved {
                bookmark,
                existing: true,
            });
        }
        let mut entropy = Entropy::from_system();
        let id = loop {
            let id = ItemId::new(&mut entropy);
            if !self.files.exists(&self.path(&id)) {
                break id;
            }
        };
        let cover = cover
            .and_then(|bytes| Some((bytes, extension(bytes)?)))
            .map(|(bytes, extension)| {
                (bytes, format!("{}.{extension}", ItemId::new(&mut entropy)))
            });
        if let Some((bytes, name)) = &cover {
            self.files.write_bytes(&self.root.join(name), bytes)?;
        }
        let one_line = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");
        let title = [one_line(title), one_line(&details.title)]
            .into_iter()
            .find(|t| !t.is_empty())
            .unwrap_or_else(|| address.clone());
        let mut file = Document::default();
        file.set_scalar("id", id.as_str());
        file.set_scalar(ADDRESS, &address);
        file.set_scalar(TITLE, title);
        for (key, value) in [
            (SITE, one_line(&details.site)),
            (COVER, cover.map(|(_, name)| name).unwrap_or_default()),
            (EXCERPT, one_line(&details.excerpt)),
            (AUTHOR, one_line(&details.author)),
            (PUBLISHED, one_line(&details.published)),
        ] {
            if !value.is_empty() {
                file.set_scalar(key, value);
            }
        }
        file.set_scalar(SAVED, iso8601(now_ms()));
        self.files.write(&self.path(&id), &file.render())?;
        Ok(Saved {
            bookmark: bookmark_of(&file, &id),
            existing: false,
        })
    }

    /// Every Bookmark, newest Saved first. Files that are no Bookmark are skipped.
    pub fn list(&self) -> io::Result<Vec<Bookmark>> {
        let mut all = Vec::new();
        for listed in self.files.list(&self.root)? {
            let Some(id) = (listed.name.strip_suffix(".md")).and_then(ItemId::parse) else {
                continue;
            };
            if let Some(file) = self.read_file(&id) {
                all.push(bookmark_of(&file, &id));
            }
        }
        all.sort_by(|a, b| b.saved.cmp(&a.saved).then_with(|| a.id.cmp(&b.id)));
        Ok(all)
    }

    /// The Bookmark `id`; `NotFound` if there is none.
    pub fn read(&self, id: &str) -> io::Result<Bookmark> {
        let id = ItemId::parse(id).ok_or(io::ErrorKind::NotFound)?;
        let file = self.read_file(&id).ok_or(io::ErrorKind::NotFound)?;
        Ok(bookmark_of(&file, &id))
    }

    /// The bytes of the Cover of the Bookmark `id`; `NotFound` if there is
    /// no such Bookmark or it has no Cover.
    pub fn read_cover(&self, id: &str) -> io::Result<Vec<u8>> {
        let name = self.read(id)?.cover.ok_or(io::ErrorKind::NotFound)?;
        self.files.read_bytes(&self.root.join(name))
    }

    /// Deletes the Bookmark `id` with its Cover; one that is gone already is as wanted.
    pub fn delete(&self, id: &str) -> io::Result<()> {
        let Ok(bookmark) = self.read(id) else {
            return Ok(());
        };
        if let Some(cover) = bookmark.cover.filter(|name| is_image_file(name)) {
            ignore_not_found(self.files.remove(&self.root.join(cover)))?;
        }
        ignore_not_found(
            self.files
                .remove(&self.root.join(format!("{}.md", bookmark.id))),
        )
    }

    fn path(&self, id: &ItemId) -> PathBuf {
        self.root.join(format!("{id}.md"))
    }

    /// The file of `id`, none if it is none, which includes a file whose
    /// front matter names another `id` or no Address.
    fn read_file(&self, id: &ItemId) -> Option<Document> {
        let text = self.files.read(&self.path(id)).ok()?;
        Document::parse(&text)
            .filter(|file| file.scalar("id") == Some(id.as_str()) && file.scalar(ADDRESS).is_some())
    }
}

fn ignore_not_found(result: io::Result<()>) -> io::Result<()> {
    match result {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

fn bookmark_of(file: &Document, id: &ItemId) -> Bookmark {
    let text = |key| file.scalar(key).unwrap_or_default().to_owned();
    Bookmark {
        id: id.as_str().to_owned(),
        address: text(ADDRESS),
        title: text(TITLE),
        site: text(SITE),
        cover: file
            .scalar(COVER)
            .filter(|c| is_image_file(c))
            .map(str::to_owned),
        excerpt: text(EXCERPT),
        author: text(AUTHOR),
        published: text(PUBLISHED),
        saved: text(SAVED),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n rest";

    /// A Bookmarks folder in a temporary directory, removed afterwards.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Self {
            let id = ItemId::new(&mut Entropy::from_system());
            Self(
                std::env::temp_dir()
                    .join(format!("pkm-bookmarks-{id}"))
                    .join("Bookmarks"),
            )
        }

        fn store(&self) -> Bookmarks {
            Bookmarks::open(&self.0).unwrap()
        }

        fn files(&self) -> Vec<String> {
            let mut names: Vec<_> = fs::read_dir(&self.0)
                .unwrap()
                .map(|e| e.unwrap().file_name().into_string().unwrap())
                .collect();
            names.sort();
            names
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            drop(fs::remove_dir_all(self.0.parent().unwrap()));
        }
    }

    fn details(title: &str) -> Details {
        Details {
            title: title.into(),
            site: "Site".into(),
            excerpt: "About".into(),
            author: "Ann".into(),
            published: "2024-05-01".into(),
            ..Details::default()
        }
    }

    #[test]
    fn a_saved_page_is_a_markdown_file_named_by_its_id_with_its_details() {
        let scratch = Scratch::new();
        let saved = scratch
            .store()
            .save(
                "https://Example.com/a?utm_source=x#top",
                &details("Page"),
                "",
                None,
            )
            .unwrap();
        assert!(!saved.existing);
        let b = saved.bookmark;
        assert_eq!(b.address, "https://example.com/a");
        assert_eq!(
            (b.title.as_str(), b.site.as_str(), b.author.as_str()),
            ("Page", "Site", "Ann")
        );
        let text = fs::read_to_string(scratch.0.join(format!("{}.md", b.id))).unwrap();
        assert!(text.starts_with(&format!(
            "---\nid: {}\naddress: https://example.com/a\ntitle: Page\nsite: Site\nexcerpt: About\nauthor: Ann\npublished: 2024-05-01\nsaved: 20",
            b.id
        )));
        assert!(text.ends_with("Z\n---\n"));
        assert_eq!(scratch.store().read(&b.id).unwrap(), b);
    }

    #[test]
    fn the_users_title_wins_and_a_blank_one_falls_back_to_the_page_then_the_address() {
        let scratch = Scratch::new();
        let store = scratch.store();
        let title = |address: &str, details: &Details, typed: &str| {
            store
                .save(address, details, typed, None)
                .unwrap()
                .bookmark
                .title
        };
        assert_eq!(
            title("https://a.test/1", &details("Page"), "Mine\n  now"),
            "Mine now"
        );
        assert_eq!(title("https://a.test/2", &details("Page"), "  "), "Page");
        assert_eq!(
            title("https://a.test/3", &Details::default(), ""),
            "https://a.test/3"
        );
    }

    #[test]
    fn saving_an_address_saved_already_gives_that_bookmark() {
        let scratch = Scratch::new();
        let store = scratch.store();
        let first = store
            .save("https://example.com/a", &details("One"), "", Some(PNG))
            .unwrap();
        let again = store
            .save(
                "HTTPS://example.com/a?utm_medium=mail#x",
                &details("Two"),
                "",
                None,
            )
            .unwrap();
        assert!(again.existing);
        assert_eq!(again.bookmark, first.bookmark);
        assert_eq!(store.list().unwrap().len(), 1);
    }

    #[test]
    fn an_address_that_is_none_is_refused() {
        let scratch = Scratch::new();
        let error = scratch
            .store()
            .save("ftp://x.test/a", &details("X"), "", None)
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(scratch.files().is_empty());
    }

    #[test]
    fn the_list_is_newest_saved_first_and_skips_other_files() {
        let scratch = Scratch::new();
        let store = scratch.store();
        for (id, saved) in [
            ("aaaaaaaaaaaa", "2024-01-01T00:00:00Z"),
            ("bbbbbbbbbbbb", "2024-03-01T00:00:00Z"),
            ("cccccccccccc", "2024-02-01T00:00:00Z"),
        ] {
            let text = format!(
                "---\nid: {id}\naddress: https://x.test/{id}\ntitle: {id}\nsaved: {saved}\n---\n"
            );
            fs::write(scratch.0.join(format!("{id}.md")), text).unwrap();
        }
        fs::write(scratch.0.join("notes.md"), "x").unwrap();
        fs::write(
            scratch.0.join("dddddddddddd.md"),
            "---\nid: eeeeeeeeeeee\n---\n",
        )
        .unwrap();
        let ids: Vec<_> = store.list().unwrap().into_iter().map(|b| b.id).collect();
        assert_eq!(ids, ["bbbbbbbbbbbb", "cccccccccccc", "aaaaaaaaaaaa"]);
    }

    #[test]
    fn the_cover_is_kept_beside_the_bookmark_and_read_back() {
        let scratch = Scratch::new();
        let store = scratch.store();
        let b = store
            .save("https://a.test/", &details("A"), "", Some(PNG))
            .unwrap()
            .bookmark;
        let cover = b.cover.clone().unwrap();
        assert!(is_image_file(&cover) && cover.ends_with(".png"));
        assert!(scratch.files().contains(&cover));
        assert_eq!(store.read_cover(&b.id).unwrap(), PNG);
    }

    #[test]
    fn a_cover_that_is_no_image_is_left_out_without_failing_the_save() {
        let scratch = Scratch::new();
        let store = scratch.store();
        let b = store
            .save("https://a.test/", &details("A"), "", Some(b"<html>"))
            .unwrap()
            .bookmark;
        assert_eq!(b.cover, None);
        assert_eq!(
            store.read_cover(&b.id).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        assert_eq!(scratch.files().len(), 1);
    }

    #[test]
    fn deleting_takes_the_cover_and_deleting_again_is_fine() {
        let scratch = Scratch::new();
        let store = scratch.store();
        let keep = store
            .save("https://a.test/keep", &details("Keep"), "", Some(PNG))
            .unwrap()
            .bookmark;
        let gone = store
            .save("https://a.test/gone", &details("Gone"), "", Some(PNG))
            .unwrap()
            .bookmark;
        store.delete(&gone.id).unwrap();
        store.delete(&gone.id).unwrap();
        assert_eq!(
            store.read(&gone.id).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        assert_eq!(scratch.files().len(), 2);
        assert_eq!(store.list().unwrap(), [keep]);
    }
}
