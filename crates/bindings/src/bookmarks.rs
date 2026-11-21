//! The Bookmarks surface of the UniFFI library (ADR-0007): the Bookmarks store
//! and the extraction of a page's details. The logic lives in `pkm-bookmarks`.

use pkm_bookmarks::{Article, Bookmark, Bookmarks, Details, Saved};
use std::collections::HashMap;

/// Why the Bookmarks couldn't be read or saved.
#[derive(Debug, uniffi::Error)]
#[uniffi(flat_error)]
pub enum BookmarksError {
    Failed(String),
}

impl std::fmt::Display for BookmarksError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self::Failed(message) = self;
        f.write_str(message)
    }
}

impl From<std::io::Error> for BookmarksError {
    fn from(e: std::io::Error) -> Self {
        Self::Failed(e.to_string())
    }
}

/// A Bookmark and its details, see `pkm_bookmarks::Bookmark`.
#[uniffi::remote(Record)]
pub struct Bookmark {
    pub id: String,
    pub address: String,
    pub title: String,
    pub site: String,
    pub cover: Option<String>,
    pub excerpt: String,
    pub author: String,
    pub published: String,
    pub saved: String,
    pub has_article: bool,
    pub tags: Vec<String>,
}

/// The text of a page, see `pkm_bookmarks::Article`.
#[uniffi::remote(Record)]
pub struct Article {
    pub markdown: String,
    pub reads_like_article: bool,
    pub images: Vec<String>,
}

/// What a page says about itself, see `pkm_bookmarks::Details`.
#[uniffi::remote(Record)]
pub struct Details {
    pub title: String,
    pub site: String,
    pub cover: Option<String>,
    pub excerpt: String,
    pub author: String,
    pub published: String,
}

/// What Saving gave, see `pkm_bookmarks::Saved`.
#[uniffi::remote(Record)]
pub struct Saved {
    pub bookmark: Bookmark,
    pub existing: bool,
}

/// The Details of the page at `address` whose HTML is `html`; pass an empty
/// `html` where the download failed, for the Details of the Address alone.
#[uniffi::export]
pub fn extract_details(html: String, address: String) -> Details {
    pkm_bookmarks::extract(&html, &address)
}

/// The Article of the page at `address` whose HTML is `html`; empty where the page has no text.
#[uniffi::export]
pub fn read_article(html: String, address: String) -> Article {
    pkm_bookmarks::read_article(&html, &address)
}

/// The Address `address` stands for, without tracking parameters and `#` part;
/// none if it is no `http` or `https` address.
#[uniffi::export]
pub fn normalise_address(address: String) -> Option<String> {
    pkm_bookmarks::normalise(&address)
}

/// Whether the Address names a PDF or EPUB, which has no page to download.
#[uniffi::export]
pub fn is_document_address(address: String) -> bool {
    pkm_bookmarks::is_document(&address)
}

/// The Bookmarks in a folder, see `pkm_bookmarks::Bookmarks`.
#[derive(uniffi::Object)]
pub struct BookmarkStore(Bookmarks);

#[uniffi::export]
impl BookmarkStore {
    /// Opens the Bookmarks in the folder `root`, creating it if needed.
    #[uniffi::constructor]
    pub fn open(root: String) -> Result<Self, BookmarksError> {
        Ok(Self(Bookmarks::open(root)?))
    }

    /// Saves the page at `address` with the user's `title` and `tags` unless it is Saved already,
    /// which gives that Bookmark unchanged. A blank `title` leaves it to the page's; invalid Tags are
    /// left out. `article` is the Article's Markdown if it is to have one; `images` the bytes
    /// downloaded for `Details.cover` and the addresses in `Article.images`, by address: those that
    /// are missing are left out.
    pub fn save(
        &self,
        address: String,
        details: Details,
        title: String,
        tags: Vec<String>,
        article: Option<String>,
        images: HashMap<String, Vec<u8>>,
    ) -> Result<Saved, BookmarksError> {
        Ok(self.0.save(
            &address,
            &details,
            &title,
            &tags,
            article.as_deref(),
            &images,
        )?)
    }

    /// Every Bookmark, newest Saved first.
    pub fn bookmarks(&self) -> Result<Vec<Bookmark>, BookmarksError> {
        Ok(self.0.list()?)
    }

    /// Every Bookmark carrying the Tag, newest Saved first.
    pub fn bookmarks_tagged(&self, tag: String) -> Result<Vec<Bookmark>, BookmarksError> {
        Ok(self.0.list_tagged(&tag)?)
    }

    pub fn read(&self, id: String) -> Result<Bookmark, BookmarksError> {
        Ok(self.0.read(&id)?)
    }

    /// The bytes of the Bookmark's Cover.
    pub fn read_cover(&self, id: String) -> Result<Vec<u8>, BookmarksError> {
        Ok(self.0.read_cover(&id)?)
    }

    /// The Markdown of the Bookmark's Article.
    pub fn read_article(&self, id: String) -> Result<String, BookmarksError> {
        Ok(self.0.read_article(&id)?)
    }

    /// The bytes of the image `name` that the Bookmark's Article shows.
    pub fn read_image(&self, id: String, name: String) -> Result<Vec<u8>, BookmarksError> {
        Ok(self.0.read_image(&id, &name)?)
    }

    /// Renames the Bookmark; a blank `title` changes nothing.
    pub fn rename(&self, id: String, title: String) -> Result<Bookmark, BookmarksError> {
        Ok(self.0.rename(&id, &title)?)
    }

    /// Puts a Tag on the Bookmark; fails for an invalid Tag.
    pub fn add_tag(&self, id: String, tag: String) -> Result<Bookmark, BookmarksError> {
        Ok(self.0.add_tag(&id, &tag)?)
    }

    /// Takes a Tag off the Bookmark; the Tag stays known.
    pub fn remove_tag(&self, id: String, tag: String) -> Result<Bookmark, BookmarksError> {
        Ok(self.0.remove_tag(&id, &tag)?)
    }

    /// Takes a Tag off every Bookmark, for Delete a Tag.
    pub fn remove_tag_everywhere(&self, tag: String) -> Result<(), BookmarksError> {
        Ok(self.0.remove_tag_everywhere(&tag)?)
    }

    /// Gives a Bookmark without an Article the `article`, with its `images` as in `save`.
    /// A Bookmark that has one keeps it.
    pub fn add_article(
        &self,
        id: String,
        article: String,
        images: HashMap<String, Vec<u8>>,
    ) -> Result<Bookmark, BookmarksError> {
        Ok(self.0.add_article(&id, &article, &images)?)
    }

    /// Takes the Article and its images off the Bookmark.
    pub fn drop_article(&self, id: String) -> Result<Bookmark, BookmarksError> {
        Ok(self.0.drop_article(&id)?)
    }

    /// Deletes the Bookmark with its Cover and images.
    pub fn delete(&self, id: String) -> Result<(), BookmarksError> {
        Ok(self.0.delete(&id)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_saved_through_the_bindings_is_listed() {
        let dir =
            std::env::temp_dir().join(format!("pkm-bindings-bookmarks-{}", std::process::id()));
        let store =
            BookmarkStore::open(dir.join("Bookmarks").to_string_lossy().into_owned()).unwrap();
        let address = "https://example.com/a?utm_source=x".to_owned();
        let details = extract_details("<title>Hi</title>".into(), address.clone());
        let page = "<p>Some text, long enough to be kept.</p>";
        let article = read_article(page.into(), address.clone());
        let saved = store
            .save(
                address,
                details,
                String::new(),
                vec!["read".into()],
                Some(article.markdown),
                HashMap::new(),
            )
            .unwrap();
        let rows = store.bookmarks().unwrap();
        assert_eq!(
            (
                rows[0].title.as_str(),
                rows[0].address.as_str(),
                &rows[0].tags[..],
                saved.existing
            ),
            (
                "Hi",
                "https://example.com/a",
                &["read".to_owned()][..],
                false
            )
        );
        assert_eq!(
            store.read_article(rows[0].id.clone()).unwrap(),
            "Some text, long enough to be kept.\n"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
