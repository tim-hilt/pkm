//! The Bookmarks surface of the wasm exports, mirroring `crates/bindings/src/bookmarks.rs`
//! for what the web app does: Save, list, Tags, Article.
//!
//! The store works on files in memory as the Notes' does, but the client does
//! not hand in the images: it reads them from its storage when shown.

use pkm_bookmarks::{Bookmarks, MemoryFiles};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use wasm_bindgen::prelude::*;

fn error(e: impl std::fmt::Display) -> JsError {
    JsError::new(&e.to_string())
}

/// A Bookmark and its details, see `pkm_bookmarks::Bookmark`.
#[wasm_bindgen(getter_with_clone)]
#[derive(Clone)]
pub struct Bookmark {
    pub id: String,
    /// Normalised.
    pub address: String,
    pub title: String,
    pub site: String,
    /// The file name of the Cover beside the Bookmark.
    pub cover: Option<String>,
    pub excerpt: String,
    pub author: String,
    pub published: String,
    /// UTC ISO 8601.
    pub saved: String,
    #[wasm_bindgen(js_name = hasArticle)]
    pub has_article: bool,
    pub tags: Vec<String>,
    /// How much of the Article was read, `0..=100`; none if it never was.
    pub percent: Option<u8>,
}

impl From<pkm_bookmarks::Bookmark> for Bookmark {
    fn from(b: pkm_bookmarks::Bookmark) -> Self {
        Self {
            id: b.id,
            address: b.address,
            title: b.title,
            site: b.site,
            cover: b.cover,
            excerpt: b.excerpt,
            author: b.author,
            published: b.published,
            saved: b.saved,
            has_article: b.has_article,
            tags: b.tags,
            percent: b.progress.map(|p| p.percent),
        }
    }
}

/// What a page says about itself, see `pkm_bookmarks::Details`.
#[wasm_bindgen(getter_with_clone)]
pub struct Details {
    pub title: String,
    pub site: String,
    pub cover: Option<String>,
    pub excerpt: String,
    pub author: String,
    pub published: String,
}

impl From<pkm_bookmarks::Details> for Details {
    fn from(d: pkm_bookmarks::Details) -> Self {
        Self {
            title: d.title,
            site: d.site,
            cover: d.cover,
            excerpt: d.excerpt,
            author: d.author,
            published: d.published,
        }
    }
}

impl From<&Details> for pkm_bookmarks::Details {
    fn from(d: &Details) -> Self {
        Self {
            title: d.title.clone(),
            site: d.site.clone(),
            cover: d.cover.clone(),
            excerpt: d.excerpt.clone(),
            author: d.author.clone(),
            published: d.published.clone(),
        }
    }
}

/// The text of a page, see `pkm_bookmarks::Article`.
#[wasm_bindgen(getter_with_clone)]
pub struct Article {
    pub markdown: String,
    #[wasm_bindgen(js_name = readsLikeArticle)]
    pub reads_like_article: bool,
    /// The absolute addresses of its images.
    pub images: Vec<String>,
}

/// What Saving gave, see `pkm_bookmarks::Saved`.
#[wasm_bindgen(getter_with_clone)]
pub struct Saved {
    pub bookmark: Bookmark,
    /// Its Address was Saved already: this is that Bookmark, unchanged.
    pub existing: bool,
}

/// A Bookmark that Search found, see `pkm_bookmarks::Found`.
#[wasm_bindgen(getter_with_clone)]
pub struct BookmarkFound {
    pub bookmark: Bookmark,
    /// Around the match, as written.
    pub snippet: String,
    /// The match in `snippet`, in UTF-16 units: `matchedStart` up to `matchedEnd`.
    #[wasm_bindgen(js_name = matchedStart)]
    pub matched_start: u32,
    #[wasm_bindgen(js_name = matchedEnd)]
    pub matched_end: u32,
}

/// Downloaded images by their address, for `save` and `addArticle`.
#[wasm_bindgen]
#[derive(Default)]
pub struct Images(HashMap<String, Vec<u8>>);

#[wasm_bindgen]
impl Images {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Images {
        Images::default()
    }

    pub fn add(&mut self, address: String, bytes: Vec<u8>) {
        self.0.insert(address, bytes);
    }
}

/// The Details of the page at `address` whose HTML is `html`; pass an empty `html` for the Details of the Address alone.
#[wasm_bindgen(js_name = extractDetails)]
pub fn extract_details(html: &str, address: &str) -> Details {
    pkm_bookmarks::extract(html, address).into()
}

/// The Article of the page at `address` whose HTML is `html`; empty where the page has no text.
#[wasm_bindgen(js_name = readArticle)]
pub fn read_article(html: &str, address: &str) -> Article {
    let article = pkm_bookmarks::read_article(html, address);
    Article {
        markdown: article.markdown,
        reads_like_article: article.reads_like_article,
        images: article.images,
    }
}

/// The Address without tracking parameters and `#` part; none if it is no `http` or `https` address.
#[wasm_bindgen(js_name = normaliseAddress)]
pub fn normalise_address(address: &str) -> Option<String> {
    pkm_bookmarks::normalise(address)
}

/// Whether the Address names a PDF or EPUB, which has no page to download.
#[wasm_bindgen(js_name = isDocumentAddress)]
pub fn is_document_address(address: &str) -> bool {
    pkm_bookmarks::is_document(address)
}

/// The Bookmarks, see `pkm_bookmarks::Bookmarks`.
#[wasm_bindgen]
pub struct BookmarkStore {
    files: Arc<MemoryFiles>,
    bookmarks: Bookmarks,
}

#[wasm_bindgen]
impl BookmarkStore {
    /// The folder the Bookmarks are kept in, in the app's storage.
    pub fn folder() -> String {
        Bookmarks::FOLDER.to_owned()
    }

    /// Opens the Bookmarks from the saved text files: `paths[i]` held `texts[i]`, in storage paths
    /// (`Bookmarks/<id>.md`, `Bookmarks/.state/<id>.json`, `Tags/tags.json`). Images are not held.
    #[wasm_bindgen(constructor)]
    pub fn new(paths: Vec<String>, texts: Vec<String>) -> BookmarkStore {
        let files = Arc::new(MemoryFiles::default());
        for (path, text) in paths.into_iter().zip(texts) {
            files.load(Path::new(&path), text);
        }
        let bookmarks = Bookmarks::open_in(files.clone(), Bookmarks::FOLDER);
        Self { files, bookmarks }
    }

    /// Puts a text file there as it was saved, e.g. the Tags file that Tasks changed; not a change.
    #[wasm_bindgen(js_name = loadFile)]
    pub fn load_file(&self, path: &str, text: String) {
        self.files.load(Path::new(path), text);
    }

    /// The paths written or removed since the last call; `fileBytes` holds what to save, none for a removed one.
    #[wasm_bindgen(js_name = takeChanges)]
    pub fn take_changes(&self) -> Vec<String> {
        (self.files.take_changes().into_iter())
            .map(|p| p.to_string_lossy().into_owned())
            .collect()
    }

    /// What the file at `path` holds now; none if there is no such file.
    #[wasm_bindgen(js_name = fileBytes)]
    pub fn file_bytes(&self, path: &str) -> Option<Vec<u8>> {
        use pkm_bookmarks::Files;
        self.files.read_bytes(Path::new(path)).ok()
    }

    /// Saves the page at `address` unless it is Saved already, see `pkm_bookmarks::Bookmarks::save`.
    /// `article` is the Article's Markdown if it is to have one.
    pub fn save(
        &self,
        address: &str,
        details: &Details,
        title: &str,
        tags: Vec<String>,
        article: Option<String>,
        images: &Images,
    ) -> Result<Saved, JsError> {
        let saved = (self.bookmarks)
            .save(
                address,
                &details.into(),
                title,
                &tags,
                article.as_deref(),
                &images.0,
            )
            .map_err(error)?;
        Ok(Saved {
            bookmark: saved.bookmark.into(),
            existing: saved.existing,
        })
    }

    /// Every Bookmark, newest Saved first.
    pub fn bookmarks(&self) -> Result<Vec<Bookmark>, JsError> {
        let rows = self.bookmarks.list().map_err(error)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    /// Every Bookmark carrying the Tag, newest Saved first.
    #[wasm_bindgen(js_name = bookmarksTagged)]
    pub fn bookmarks_tagged(&self, tag: &str) -> Result<Vec<Bookmark>, JsError> {
        let rows = self.bookmarks.list_tagged(tag).map_err(error)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    /// The Bookmarks with `query` in their Title, Site, Address or Article, Title matches first.
    pub fn search(&self, query: &str) -> Result<Vec<BookmarkFound>, JsError> {
        let found = self.bookmarks.search(query).map_err(error)?;
        Ok(found
            .into_iter()
            .map(|f| BookmarkFound {
                bookmark: f.bookmark.into(),
                snippet: f.snippet.text,
                matched_start: f.snippet.matched.start as u32,
                matched_end: f.snippet.matched.end as u32,
            })
            .collect())
    }

    pub fn read(&self, id: &str) -> Result<Bookmark, JsError> {
        self.bookmarks.read(id).map(Into::into).map_err(error)
    }

    /// The Markdown of the Bookmark's Article.
    #[wasm_bindgen(js_name = readArticle)]
    pub fn read_article(&self, id: &str) -> Result<String, JsError> {
        self.bookmarks.read_article(id).map_err(error)
    }

    /// Renames the Bookmark; a blank `title` changes nothing.
    pub fn rename(&self, id: &str, title: &str) -> Result<Bookmark, JsError> {
        (self.bookmarks.rename(id, title))
            .map(Into::into)
            .map_err(error)
    }

    /// Puts a Tag on the Bookmark; a new Tag joins the Tags file.
    #[wasm_bindgen(js_name = addTag)]
    pub fn add_tag(&self, id: &str, tag: &str) -> Result<Bookmark, JsError> {
        (self.bookmarks.add_tag(id, tag))
            .map(Into::into)
            .map_err(error)
    }

    /// Takes a Tag off the Bookmark.
    #[wasm_bindgen(js_name = removeTag)]
    pub fn remove_tag(&self, id: &str, tag: &str) -> Result<Bookmark, JsError> {
        (self.bookmarks.remove_tag(id, tag))
            .map(Into::into)
            .map_err(error)
    }

    /// Takes a Tag off every Bookmark, for Delete a Tag.
    #[wasm_bindgen(js_name = removeTagEverywhere)]
    pub fn remove_tag_everywhere(&self, tag: &str) -> Result<(), JsError> {
        self.bookmarks.remove_tag_everywhere(tag).map_err(error)
    }

    /// Gives a Bookmark without an Article the `article`, with its `images` as in `save`.
    #[wasm_bindgen(js_name = addArticle)]
    pub fn add_article(
        &self,
        id: &str,
        article: &str,
        images: &Images,
    ) -> Result<Bookmark, JsError> {
        (self.bookmarks.add_article(id, article, &images.0))
            .map(Into::into)
            .map_err(error)
    }

    /// Takes the Article and its images off the Bookmark.
    #[wasm_bindgen(js_name = dropArticle)]
    pub fn drop_article(&self, id: &str) -> Result<Bookmark, JsError> {
        (self.bookmarks.drop_article(id))
            .map(Into::into)
            .map_err(error)
    }

    /// Deletes the Bookmark with its Cover, images and state.
    pub fn delete(&self, id: &str) -> Result<(), JsError> {
        self.bookmarks.delete(id).map_err(error)
    }
}
