//! The Bookmarks store: `<id>.md` files with the front matter `id`,
//! `address`, `title`, `site`, `cover`, `excerpt`, `author`, `published` and
//! `saved`, beside their Cover `<id>.<ext>` (Bookmarks ADR-0002). The body is
//! the Article, if the Bookmark has one.

use crate::address::normalise;
use crate::extract::Details;
use crate::images;
use pkm_base::clock::{iso8601, now_ms};
use pkm_base::files::{DiskFiles, Files};
use pkm_base::front_matter::Document;
use pkm_base::images::{extension, is_image_file};
use pkm_base::item_id::ItemId;
use pkm_base::random::Entropy;
use std::collections::HashMap;
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
    /// Whether the Bookmark has an Article.
    pub has_article: bool,
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
    /// WebP, so that a broken image never costs the Bookmark. `article` is the
    /// Article's Markdown (see `read_article`) if the Bookmark is to have one;
    /// blank is none. `images` holds the bytes downloaded for the addresses
    /// the Article names: those are kept beside the Bookmark and the Article
    /// points at the files; an image that wasn't downloaded or is no PNG, JPEG,
    /// GIF or WebP is dropped from the Article. `InvalidInput` for an
    /// `address` that is none (see `normalise`).
    pub fn save(
        &self,
        address: &str,
        details: &Details,
        title: &str,
        cover: Option<&[u8]>,
        article: Option<&str>,
        images: &HashMap<String, Vec<u8>>,
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
        let cover = match cover {
            Some(bytes) => self.keep_image(&mut entropy, bytes)?,
            None => None,
        };
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
            (COVER, cover.unwrap_or_default()),
            (EXCERPT, one_line(&details.excerpt)),
            (AUTHOR, one_line(&details.author)),
            (PUBLISHED, one_line(&details.published)),
        ] {
            if !value.is_empty() {
                file.set_scalar(key, value);
            }
        }
        file.set_scalar(SAVED, iso8601(now_ms()));
        if let Some(article) = article {
            let mut kept: HashMap<String, Option<String>> = HashMap::new();
            let mut failed = None;
            let article = images::rewrite(article, |address| {
                kept.entry(address.to_owned())
                    .or_insert_with(|| {
                        let bytes = images.get(address)?;
                        self.keep_image(&mut entropy, bytes).unwrap_or_else(|e| {
                            failed = Some(e);
                            None
                        })
                    })
                    .clone()
            });
            if let Some(error) = failed {
                return Err(error);
            }
            let article = article.trim();
            if !article.is_empty() {
                file.body = format!("{article}\n");
            }
        }
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

    /// The Article of the Bookmark `id`; `NotFound` if there is no such
    /// Bookmark or it has no Article.
    pub fn read_article(&self, id: &str) -> io::Result<String> {
        let id = ItemId::parse(id).ok_or(io::ErrorKind::NotFound)?;
        let file = self.read_file(&id).ok_or(io::ErrorKind::NotFound)?;
        Some(file.body)
            .filter(|b| !b.trim().is_empty())
            .ok_or(io::ErrorKind::NotFound.into())
    }

    /// The bytes of the image `name`, as the Article of the Bookmark `id`
    /// names it; `NotFound` if there is no such Bookmark, or the Article
    /// doesn't use it, or it is gone.
    pub fn read_image(&self, id: &str, name: &str) -> io::Result<Vec<u8>> {
        let bookmark = self.read(id)?;
        let used = bookmark.cover.as_deref() == Some(name)
            || self.images_of(&bookmark).iter().any(|n| n == name);
        if !used {
            return Err(io::ErrorKind::NotFound.into());
        }
        self.files.read_bytes(&self.root.join(name))
    }

    /// Deletes the Bookmark `id` with its Cover and images; one that is gone
    /// already is as wanted.
    pub fn delete(&self, id: &str) -> io::Result<()> {
        let Ok(bookmark) = self.read(id) else {
            return Ok(());
        };
        let images = self.images_of(&bookmark);
        for name in bookmark.cover.iter().chain(&images) {
            ignore_not_found(self.files.remove(&self.root.join(name)))?;
        }
        ignore_not_found(
            self.files
                .remove(&self.root.join(format!("{}.md", bookmark.id))),
        )
    }

    /// Writes `bytes` beside the Bookmarks as a new `<random-id>.<ext>`; none
    /// if they are no PNG, JPEG, GIF or WebP.
    fn keep_image(&self, entropy: &mut Entropy, bytes: &[u8]) -> io::Result<Option<String>> {
        let Some(extension) = extension(bytes) else {
            return Ok(None);
        };
        let name = format!("{}.{extension}", ItemId::new(entropy));
        self.files.write_bytes(&self.root.join(&name), bytes)?;
        Ok(Some(name))
    }

    /// The files of the images in the Bookmark's Article.
    fn images_of(&self, bookmark: &Bookmark) -> Vec<String> {
        let mut names = Vec::new();
        if let Ok(article) = self.read_article(&bookmark.id) {
            images::rewrite(&article, |name| {
                if is_image_file(name) {
                    names.push(name.to_owned());
                }
                Some(name.to_owned())
            });
        }
        names
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
        has_article: !file.body.trim().is_empty(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::article::read_article;

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
                None,
                &HashMap::new(),
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
                .save(address, details, typed, None, None, &HashMap::new())
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
            .save(
                "https://example.com/a",
                &details("One"),
                "",
                Some(PNG),
                None,
                &HashMap::new(),
            )
            .unwrap();
        let again = store
            .save(
                "HTTPS://example.com/a?utm_medium=mail#x",
                &details("Two"),
                "",
                None,
                None,
                &HashMap::new(),
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
            .save(
                "ftp://x.test/a",
                &details("X"),
                "",
                None,
                None,
                &HashMap::new(),
            )
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
            .save(
                "https://a.test/",
                &details("A"),
                "",
                Some(PNG),
                None,
                &HashMap::new(),
            )
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
            .save(
                "https://a.test/",
                &details("A"),
                "",
                Some(b"<html>"),
                None,
                &HashMap::new(),
            )
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
            .save(
                "https://a.test/keep",
                &details("Keep"),
                "",
                Some(PNG),
                None,
                &HashMap::new(),
            )
            .unwrap()
            .bookmark;
        let gone = store
            .save(
                "https://a.test/gone",
                &details("Gone"),
                "",
                Some(PNG),
                None,
                &HashMap::new(),
            )
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

    #[test]
    fn an_article_is_the_body_of_the_file_and_marks_its_bookmark() {
        let scratch = Scratch::new();
        let store = scratch.store();
        let with = store
            .save(
                "https://a.test/1",
                &details("One"),
                "",
                None,
                Some("# Head\n\nText\n\n"),
                &HashMap::new(),
            )
            .unwrap()
            .bookmark;
        let without = store
            .save(
                "https://a.test/2",
                &details("Two"),
                "",
                None,
                None,
                &HashMap::new(),
            )
            .unwrap()
            .bookmark;
        assert!(with.has_article && !without.has_article);
        assert_eq!(store.read_article(&with.id).unwrap(), "# Head\n\nText\n");
        assert_eq!(
            store.read_article(&without.id).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        let text = fs::read_to_string(scratch.0.join(format!("{}.md", with.id))).unwrap();
        assert!(text.ends_with("Z\n---\n# Head\n\nText\n"));
        let listed = store.list().unwrap();
        assert_eq!(listed.iter().filter(|b| b.has_article).count(), 1);
    }

    #[test]
    fn the_user_can_override_what_the_page_reads_like() {
        let scratch = Scratch::new();
        let store = scratch.store();
        let long = format!(
            "<article><p>{}</p></article>",
            "Plenty of prose, indeed. ".repeat(30)
        );
        let short = "<p>Home, sweet home, welcome, friends.</p>";
        let save = |address: &str, html: &str, keep: Option<bool>| {
            let article = read_article(html, address);
            let keep = keep.unwrap_or(article.reads_like_article);
            store
                .save(
                    address,
                    &details("T"),
                    "",
                    None,
                    keep.then_some(article.markdown.as_str()),
                    &HashMap::new(),
                )
                .unwrap()
                .bookmark
                .has_article
        };
        assert!(save("https://a.test/long", &long, None));
        assert!(!save("https://a.test/long-off", &long, Some(false)));
        assert!(!save("https://a.test/short", short, None));
        assert!(save("https://a.test/short-on", short, Some(true)));
    }

    #[test]
    fn a_blank_article_is_none() {
        let scratch = Scratch::new();
        let saved = scratch
            .store()
            .save(
                "https://a.test/1",
                &details("One"),
                "",
                None,
                Some(" \n"),
                &HashMap::new(),
            )
            .unwrap();
        assert!(!saved.bookmark.has_article);
    }

    #[test]
    fn deleting_takes_the_article_with_the_file() {
        let scratch = Scratch::new();
        let store = scratch.store();
        let b = store
            .save(
                "https://a.test/1",
                &details("One"),
                "",
                None,
                Some("Text"),
                &HashMap::new(),
            )
            .unwrap()
            .bookmark;
        store.delete(&b.id).unwrap();
        assert!(scratch.files().is_empty());
    }

    const JPEG: &[u8] = b"\xff\xd8\xff\xe0 rest";

    fn downloaded(pairs: &[(&str, &[u8])]) -> HashMap<String, Vec<u8>> {
        pairs
            .iter()
            .map(|(a, b)| ((*a).to_owned(), b.to_vec()))
            .collect()
    }

    fn saved_with_images(store: &Bookmarks, article: &str, images: &[(&str, &[u8])]) -> Bookmark {
        store
            .save(
                "https://x.test/a",
                &details("A"),
                "",
                None,
                Some(article),
                &downloaded(images),
            )
            .unwrap()
            .bookmark
    }

    #[test]
    fn article_images_are_kept_beside_the_bookmark_and_referenced() {
        let scratch = Scratch::new();
        let store = scratch.store();
        let article = "Intro\n\n![one](https://i.test/1.png)\n\n![two](https://i.test/2.jpg)\n\n![again](https://i.test/1.png)\n";
        let b = saved_with_images(
            &store,
            article,
            &[
                ("https://i.test/1.png", PNG),
                ("https://i.test/2.jpg", JPEG),
            ],
        );
        let text = store.read_article(&b.id).unwrap();
        let names = |ext: &str| -> Vec<String> {
            text.split("](")
                .skip(1)
                .map(|r| r.split(')').next().unwrap().to_owned())
                .filter(|n| n.ends_with(ext))
                .collect()
        };
        let (png, jpg) = (names(".png"), names(".jpg"));
        assert_eq!((png.len(), jpg.len()), (2, 1));
        assert_eq!(png[0], png[1], "one download, one file");
        assert!(!text.contains("https://i.test"));
        assert!(is_image_file(&png[0]) && is_image_file(&jpg[0]));
        assert_eq!(store.read_image(&b.id, &png[0]).unwrap(), PNG);
        assert_eq!(store.read_image(&b.id, &jpg[0]).unwrap(), JPEG);
        assert_eq!(scratch.files().len(), 3);
    }

    #[test]
    fn an_image_that_was_not_downloaded_or_is_none_is_dropped_without_failing_the_save() {
        let scratch = Scratch::new();
        let store = scratch.store();
        let article =
            "Intro\n\n![gone](https://i.test/gone.png)\n\n![bad](https://i.test/bad.png)\n\nEnd\n";
        let b = saved_with_images(&store, article, &[("https://i.test/bad.png", b"<html>")]);
        assert_eq!(store.read_article(&b.id).unwrap(), "Intro\n\nEnd\n");
        assert_eq!(scratch.files().len(), 1);
    }

    #[test]
    fn an_image_can_only_be_read_if_the_article_uses_it() {
        let scratch = Scratch::new();
        let store = scratch.store();
        let a = saved_with_images(
            &store,
            "![](https://i.test/1.png)\n",
            &[("https://i.test/1.png", PNG)],
        );
        let b = store
            .save(
                "https://x.test/b",
                &details("B"),
                "",
                None,
                None,
                &HashMap::new(),
            )
            .unwrap()
            .bookmark;
        let name = store.read_article(&a.id).unwrap();
        let name = name.trim().trim_start_matches("![](").trim_end_matches(')');
        assert!(store.read_image(&a.id, name).is_ok());
        assert!(store.read_image(&b.id, name).is_err());
        assert!(store.read_image(&a.id, "../x.png").is_err());
    }

    #[test]
    fn deleting_takes_the_article_images_and_the_cover_with_it() {
        let scratch = Scratch::new();
        let store = scratch.store();
        let b = store
            .save(
                "https://x.test/a",
                &details("A"),
                "",
                Some(PNG),
                Some("![](https://i.test/1.png)\n"),
                &downloaded(&[("https://i.test/1.png", JPEG)]),
            )
            .unwrap()
            .bookmark;
        let other = store
            .save(
                "https://x.test/b",
                &details("B"),
                "",
                None,
                Some("![](https://i.test/2.png)\n"),
                &downloaded(&[("https://i.test/2.png", PNG)]),
            )
            .unwrap()
            .bookmark;
        assert_eq!(scratch.files().len(), 5);
        store.delete(&b.id).unwrap();
        assert_eq!(scratch.files().len(), 2);
        assert!(scratch.files().iter().any(|f| f.starts_with(&other.id)));
    }
}
