//! The Bookmarks surface of the UniFFI library (ADR-0007): the Bookmarks store
//! and the extraction of a page's details. The logic lives in `pkm-bookmarks`.

use pkm_bookmarks::{Article, Bookmark, Bookmarks, Details, Saved};

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
}

/// The text of a page, see `pkm_bookmarks::Article`.
#[uniffi::remote(Record)]
pub struct Article {
    pub markdown: String,
    pub reads_like_article: bool,
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

    /// Saves the page at `address` unless it is Saved already, which gives that Bookmark.
    /// A blank `title` leaves it to the page's; `cover` is the Cover's bytes, if downloaded;
    /// `article` is the Article's Markdown if it is to have one.
    pub fn save(
        &self,
        address: String,
        details: Details,
        title: String,
        cover: Option<Vec<u8>>,
        article: Option<String>,
    ) -> Result<Saved, BookmarksError> {
        Ok(self.0.save(
            &address,
            &details,
            &title,
            cover.as_deref(),
            article.as_deref(),
        )?)
    }

    /// Every Bookmark, newest Saved first.
    pub fn bookmarks(&self) -> Result<Vec<Bookmark>, BookmarksError> {
        Ok(self.0.list()?)
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

    /// Deletes the Bookmark with its Cover.
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
                None,
                Some(article.markdown),
            )
            .unwrap();
        let rows = store.bookmarks().unwrap();
        assert_eq!(
            (
                rows[0].title.as_str(),
                rows[0].address.as_str(),
                saved.existing
            ),
            ("Hi", "https://example.com/a", false)
        );
        assert_eq!(
            store.read_article(rows[0].id.clone()).unwrap(),
            "Some text, long enough to be kept.\n"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
