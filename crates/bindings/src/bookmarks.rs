//! The Bookmarks surface of the UniFFI library (ADR-0007): the Bookmarks store
//! and the extraction of a page's details. The logic lives in `pkm-bookmarks`.

use pkm_base::markup::Rgb;
use pkm_bookmarks::{
    Anchor, Article, ArticleProgress, Bookmark, Bookmarks, Details, Highlight, Markup, PageComment,
    Place, Saved,
};
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
    pub progress: Option<ArticleProgress>,
}

/// Where an Article was left, see `pkm_bookmarks::ArticleProgress`.
#[uniffi::remote(Record)]
pub struct ArticleProgress {
    pub offset: u64,
    pub last_read: u64,
    pub percent: u8,
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

/// A passage of an Article marked in a color; its offsets are UTF-16, into the Article's Markdown.
#[derive(uniffi::Record)]
pub struct ArticleHighlight {
    pub id: String,
    pub start: u64,
    pub end: u64,
    pub quoted_text: String,
    pub color: Rgb,
    pub created: u64,
    pub comment: Option<String>,
}

/// A Comment on a spot of an Article, `offset` being UTF-16, into its Markdown.
#[derive(uniffi::Record)]
pub struct ArticleComment {
    pub id: String,
    pub offset: u64,
    pub created: u64,
    pub text: String,
}

/// Everything the user marked up on an Article, oldest first.
#[derive(uniffi::Record)]
pub struct ArticleMarkup {
    pub highlights: Vec<ArticleHighlight>,
    pub page_comments: Vec<ArticleComment>,
}

impl From<Highlight> for ArticleHighlight {
    fn from(h: Highlight) -> Self {
        Self {
            id: h.id,
            start: h.anchor.start,
            end: h.anchor.end,
            quoted_text: h.quoted_text,
            color: h.color,
            created: h.created,
            comment: h.comment,
        }
    }
}

impl From<PageComment> for ArticleComment {
    fn from(c: PageComment) -> Self {
        Self {
            id: c.id,
            offset: c.place.offset,
            created: c.created,
            text: c.text,
        }
    }
}

impl From<Markup> for ArticleMarkup {
    fn from(m: Markup) -> Self {
        Self {
            highlights: m.highlights.into_iter().map(Into::into).collect(),
            page_comments: m.page_comments.into_iter().map(Into::into).collect(),
        }
    }
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

    /// Keeps `offset` (UTF-16, into the Article's Markdown) as where the Bookmark was left, and now as
    /// its Last Read; fails for a Bookmark without an Article.
    pub fn save_progress(&self, id: String, offset: u64) -> Result<Bookmark, BookmarksError> {
        Ok(self.0.save_progress(&id, offset)?)
    }

    /// The Markup of the Bookmark's Article; fails for a Bookmark without one.
    pub fn markup(&self, id: String) -> Result<ArticleMarkup, BookmarksError> {
        Ok(self.0.markup(&id)?.into())
    }

    /// Adds a Highlight of `quoted_text` from UTF-16 offset `start` to `end` of the Article's
    /// Markdown, `created` in milliseconds since the Unix epoch; fails for an empty or out-of-range one.
    pub fn add_highlight(
        &self,
        id: String,
        start: u64,
        end: u64,
        quoted_text: String,
        color: Rgb,
        created: u64,
    ) -> Result<ArticleHighlight, BookmarksError> {
        let anchor = Anchor { start, end };
        Ok(self
            .0
            .add_highlight(&id, anchor, &quoted_text, color, created)?
            .into())
    }

    pub fn set_highlight_color(
        &self,
        id: String,
        highlight: String,
        color: Rgb,
    ) -> Result<(), BookmarksError> {
        Ok(self.0.set_highlight_color(&id, &highlight, color)?)
    }

    /// Empty `comment` deletes the Comment.
    pub fn set_highlight_comment(
        &self,
        id: String,
        highlight: String,
        comment: String,
    ) -> Result<(), BookmarksError> {
        Ok(self.0.set_highlight_comment(&id, &highlight, &comment)?)
    }

    pub fn remove_highlight(&self, id: String, highlight: String) -> Result<(), BookmarksError> {
        Ok(self.0.remove_highlight(&id, &highlight)?)
    }

    /// Adds a Comment at UTF-16 `offset`, still without text: it isn't in the Markup until
    /// `set_page_comment_text` gives it some.
    pub fn add_page_comment(
        &self,
        id: String,
        offset: u64,
        created: u64,
    ) -> Result<ArticleComment, BookmarksError> {
        Ok(self
            .0
            .add_page_comment(&id, Place { offset }, created)?
            .into())
    }

    /// Empty `text` deletes the Comment.
    pub fn set_page_comment_text(
        &self,
        id: String,
        comment: String,
        text: String,
    ) -> Result<(), BookmarksError> {
        Ok(self.0.set_page_comment_text(&id, &comment, &text)?)
    }

    pub fn remove_page_comment(&self, id: String, comment: String) -> Result<(), BookmarksError> {
        Ok(self.0.remove_page_comment(&id, &comment)?)
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
