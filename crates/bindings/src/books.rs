//! The Books surface of the UniFFI library (ADR-0007): the Library, the Reader's
//! helpers (Book Search, Table of Contents, Jump) and Markup. The logic lives in
//! `pkm-books`.

use pkm_base::highlight_colors::{HighlightColor, HighlightColors};
use pkm_books::book_text::{self, Chunk, Found};
use pkm_books::contents::{self, ContentsEntry, Entry, Position};
use pkm_books::details::{Details, Format, Size};
use pkm_books::epub;
use pkm_books::jump;
use pkm_books::library::{Book as CoreBook, DetailsPass};
use pkm_books::library::{ImportError, ImportResult, PdfProblem};
use pkm_books::listing::{self, Sort, SortKey};
use pkm_books::markup::{
    Anchor, EpubPosition, Highlight, Markup, PageComment, PageRects, Rect, Rgb,
};
use pkm_books::progress::{Place, ReadingProgress};
use pkm_books::search::{Query, WordMatch};
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

/// The error crossing the FFI: the system's message for what failed, or, for
/// an Import, why the File isn't one the app reads.
#[derive(Debug, uniffi::Error)]
#[uniffi(flat_error)]
pub enum LibraryError {
    Failed(String),
    InvalidEpub,
    DrmProtectedEpub,
    FixedLayoutEpub,
    UnsupportedFormat,
    UnreadablePdf,
    LockedPdf,
}

impl std::fmt::Display for LibraryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Failed(message) => f.write_str(message),
            Self::InvalidEpub => epub::Error::Invalid.fmt(f),
            Self::DrmProtectedEpub => epub::Error::DrmProtected.fmt(f),
            Self::FixedLayoutEpub => epub::Error::FixedLayout.fmt(f),
            Self::UnsupportedFormat => ImportError::UnsupportedFormat.fmt(f),
            Self::UnreadablePdf => ImportError::UnreadablePdf.fmt(f),
            Self::LockedPdf => ImportError::LockedPdf.fmt(f),
        }
    }
}

impl From<std::io::Error> for LibraryError {
    fn from(e: std::io::Error) -> Self {
        Self::Failed(e.to_string())
    }
}

impl From<ImportError> for LibraryError {
    fn from(e: ImportError) -> Self {
        match e {
            ImportError::Io(e) => e.into(),
            ImportError::Epub(epub::Error::Invalid) => Self::InvalidEpub,
            ImportError::Epub(epub::Error::DrmProtected) => Self::DrmProtectedEpub,
            ImportError::Epub(epub::Error::FixedLayout) => Self::FixedLayoutEpub,
            ImportError::UnsupportedFormat => Self::UnsupportedFormat,
            ImportError::UnreadablePdf => Self::UnreadablePdf,
            ImportError::LockedPdf => Self::LockedPdf,
        }
    }
}

// The core's types cross the FFI as remote types: declared here, defined (and
// booked) once in `pkm-books`, which stays free of FFI concerns.

#[uniffi::remote(Enum)]
pub enum Place {
    Pdf { page_index: u32, x: f64, y: f64 },
    Epub { spine_index: u32, offset: u64 },
}

#[uniffi::remote(Record)]
pub struct ContentsEntry {
    pub title: String,
    pub page_label: Option<String>,
    pub place: Option<Place>,
    pub sections: Vec<ContentsEntry>,
}

#[uniffi::remote(Record)]
pub struct Entry {
    pub id: u32,
    pub title: String,
    pub page_label: Option<String>,
    pub place: Option<Place>,
    pub sections: Vec<Entry>,
}

#[uniffi::remote(Record)]
pub struct Position {
    pub current: Entry,
    pub containing: Vec<Entry>,
}

#[uniffi::remote(Record)]
pub struct ReadingProgress {
    pub place: Place,
    pub last_read: u64,
}

#[uniffi::remote(Record)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[uniffi::remote(Record)]
pub struct PageRects {
    pub page_index: u32,
    pub rects: Vec<Rect>,
}

#[uniffi::remote(Record)]
pub struct EpubPosition {
    pub spine_index: u32,
    pub offset: u64,
}

#[uniffi::remote(Enum)]
pub enum Anchor {
    Pdf(Vec<PageRects>),
    Epub {
        start: EpubPosition,
        end: EpubPosition,
    },
}

#[uniffi::remote(Record)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[uniffi::remote(Record)]
pub struct HighlightColor {
    pub id: String,
    pub rgb: Rgb,
    pub is_custom: bool,
}

#[uniffi::remote(Record)]
pub struct HighlightColors {
    pub colors: Vec<HighlightColor>,
    pub default_color: HighlightColor,
}

#[uniffi::remote(Record)]
pub struct Highlight {
    pub id: String,
    pub anchor: Anchor,
    pub quoted_text: String,
    pub color: Rgb,
    pub created: u64,
    pub comment: Option<String>,
}

#[uniffi::remote(Record)]
pub struct PageComment {
    pub id: String,
    pub place: Place,
    pub created: u64,
    pub text: String,
}

#[uniffi::remote(Record)]
pub struct Markup {
    pub highlights: Vec<Highlight>,
    pub page_comments: Vec<PageComment>,
}

#[uniffi::remote(Enum)]
pub enum Format {
    Pdf,
    Epub,
}

#[uniffi::remote(Enum)]
pub enum SortKey {
    DateImported,
    Title,
    RecentlyRead,
}

#[uniffi::remote(Record)]
pub struct Sort {
    pub key: SortKey,
    pub reversed: bool,
}

/// A Book in the Library, see `pkm_books::library::Book`. Author and page count
/// are none until they are read from its PDF; an EPUB's come with its Import,
/// and its page count is always none.
#[derive(uniffi::Record)]
pub struct Book {
    pub id: String,
    pub path: String,
    /// The SHA-256 of the File, lowercase hex: a Cover made from the File
    /// holds as long as it does.
    pub sha256: String,
    pub format: Format,
    /// Date Imported, in milliseconds since the Unix epoch.
    pub imported: u64,
    pub progress: Option<ReadingProgress>,
    pub title: String,
    pub author: Option<String>,
    pub page_count: Option<u32>,
    pub percent_read: Option<u8>,
}

impl From<CoreBook> for Book {
    fn from(d: CoreBook) -> Self {
        Self {
            title: d.title(),
            percent_read: d.percent_read(),
            format: d.format(),
            id: d.id,
            path: d.path.to_string_lossy().into_owned(),
            sha256: d.sha256,
            imported: d.imported,
            progress: d.progress,
            author: d.details.as_ref().and_then(|x| x.author.clone()),
            page_count: d.details.and_then(|x| match x.size {
                Size::Pages(pages) => Some(pages),
                Size::Characters(_) => None,
            }),
        }
    }
}

#[uniffi::remote(Enum)]
pub enum WordMatch {
    Contains,
    StartsWith,
    EndsWith,
    FullWord,
}

/// What Book Search looks for, see `pkm_books::search::Query`.
#[uniffi::remote(Record)]
pub struct Query {
    pub text: String,
    pub case_sensitive: bool,
    pub word_match: WordMatch,
}

/// The text around a match, see `pkm_books::search::Excerpt`.
#[derive(uniffi::Record)]
pub struct SearchExcerpt {
    pub text: String,
    /// The match in `text`, in UTF-16 units.
    pub matched_start: u64,
    pub matched_end: u64,
}

/// A chunk of a Book's text, see `pkm_books::book_text::Chunk`.
#[derive(uniffi::Record)]
pub struct TextChunk {
    pub index: u32,
    /// Where its text starts in the whole text, in UTF-16 units.
    pub start: u64,
    /// The length of its text, in UTF-16 units.
    pub length: u64,
}

impl From<Chunk> for TextChunk {
    fn from(chunk: Chunk) -> Self {
        Self {
            index: chunk.index,
            start: chunk.start,
            length: chunk.length,
        }
    }
}

/// A match of a Book Search over a whole Book's text, see
/// `pkm_books::book_text::Found`.
#[derive(uniffi::Record)]
pub struct FoundMatch {
    /// The match's number, from 0 in reading order.
    pub number: u64,
    /// The chunk the match is in: a page's index, or a spine item's.
    pub chunk: u32,
    /// Its range in the chunk's text, in UTF-16 units.
    pub start: u64,
    pub end: u64,
    /// Its range in the whole text, in UTF-16 units.
    pub text_start: u64,
    pub text_end: u64,
    pub excerpt: SearchExcerpt,
    /// The Chapter or Section the match is in; none if the Book has no
    /// Table of Contents or the match is before its first entry.
    pub section: Option<Entry>,
}

impl From<Found> for FoundMatch {
    fn from(found: Found) -> Self {
        Self {
            number: found.number,
            chunk: found.chunk,
            start: found.range.start,
            end: found.range.end,
            text_start: found.text.start,
            text_end: found.text.end,
            excerpt: SearchExcerpt {
                text: found.excerpt.text,
                matched_start: found.excerpt.matched.start as u64,
                matched_end: found.excerpt.matched.end as u64,
            },
            section: found.section,
        }
    }
}

/// What a client's PDF engine reads out of an open PDF's pages (ADR-0003) for
/// the Book Search, see `pkm_books::book_text::PdfPages`. Implemented
/// by the client and called from the thread that searches, not the main one.
#[uniffi::export(with_foreign)]
pub trait PdfPages: Send + Sync {
    /// The text of the page at `index`; none if it has none.
    fn page_text(&self, index: u32) -> Option<String>;
    /// The number printed on the page at `index`, or its position in the File.
    fn page_number(&self, index: u32) -> String;
    /// Where a match on the page at `index`, from `start` to `end` units into
    /// its text, sits, for the Table of Contents; none if the text isn't on the
    /// page. Called once per match, in reading order.
    fn place_of_match(&self, index: u32, start: u64, end: u64) -> Option<Place>;
}

/// Adapts the client's pages to the core's.
struct Pages(Arc<dyn PdfPages>);

impl book_text::PdfPages for Pages {
    fn page_text(&self, index: u32) -> Option<String> {
        self.0.page_text(index)
    }

    fn page_number(&self, index: u32) -> String {
        self.0.page_number(index)
    }

    fn place_of_match(&self, index: u32, range: std::ops::Range<usize>) -> Option<Place> {
        self.0
            .place_of_match(index, range.start as u64, range.end as u64)
    }
}

/// A Book's text as the Book Search searches it, see
/// `pkm_books::book_text::BookText`.
#[derive(uniffi::Object)]
pub struct BookText(Arc<book_text::BookText>);

#[uniffi::export]
impl BookText {
    /// The text of `book`'s spine items, which the core reads itself (ADR-0021).
    #[uniffi::constructor]
    pub fn epub(book: Arc<EpubBook>) -> Arc<Self> {
        Arc::new(Self(Arc::new(book_text::BookText::epub(book.0.clone()))))
    }

    /// The text of a PDF of `pages` pages, which `reader` reads out (ADR-0003)
    /// as they are asked for, and the core keeps.
    #[uniffi::constructor]
    pub fn pdf(pages: u32, reader: Arc<dyn PdfPages>) -> Arc<Self> {
        Arc::new(Self(Arc::new(book_text::BookText::pdf(
            pages,
            Arc::new(Pages(reader)),
        ))))
    }

    /// The length of the whole text, in UTF-16 units. Reads every chunk of a
    /// PDF, which are then kept.
    pub fn length(&self) -> u64 {
        self.0.length()
    }

    /// The chunk at `index`; none beyond the chunks. Reads every chunk of a
    /// PDF, as `length` does.
    pub fn chunk(&self, index: u32) -> Option<TextChunk> {
        self.0.chunk(index).map(Into::into)
    }

    /// The chunk holding the character at `offset` in the whole text; none
    /// without chunks. Reads every chunk of a PDF, as `length` does.
    pub fn chunk_containing(&self, offset: u64) -> Option<TextChunk> {
        self.0.chunk_containing(offset).map(Into::into)
    }

    /// The text of the chunk at `index`; none beyond the chunks.
    pub fn text(&self, index: u32) -> Option<String> {
        self.0.text(index)
    }

    /// The number printed on the chunk at `index`; none in a Format without pages.
    pub fn page_number(&self, index: u32) -> Option<String> {
        self.0.page_number(index)
    }

    /// Begins a Book Search for `query` over the whole text, which places
    /// each match in `contents`, the Book's Table of Contents.
    pub fn search(&self, query: Query, contents: Arc<TableOfContents>) -> Arc<SearchSession> {
        Arc::new(SearchSession(self.0.search(query, contents.0.clone())))
    }
}

/// One Book Search over a `BookText`, see
/// `pkm_books::book_text::Search`. The client asks it for the next chunk's
/// matches until there are none; stopping earlier cancels the search.
#[derive(uniffi::Object)]
pub struct SearchSession(book_text::Search);

#[uniffi::export]
impl SearchSession {
    /// The matches in the next chunk of the text, in reading order; none once
    /// the whole text has been searched. A `next` meanwhile waits its turn.
    pub fn next(&self) -> Option<Vec<FoundMatch>> {
        Some(self.0.next()?.into_iter().map(Into::into).collect())
    }

    /// The number of the match that starts at `offset` in the whole text, if
    /// one has been found so far. Answers while `next` searches.
    pub fn number_at(&self, offset: u64) -> Option<u64> {
        self.0.number_at(offset)
    }
}

/// A file of an EPUB, see `pkm_books::epub::Resource`.
#[derive(uniffi::Record)]
pub struct EpubResource {
    pub bytes: Vec<u8>,
    pub media_type: String,
}

/// An EPUB opened for reading, see `pkm_books::epub::Epub`. Clients keep it
/// open while the Reader shows the book, instead of opening it per file.
#[derive(uniffi::Object)]
pub struct EpubBook(Arc<epub::Epub>);

#[uniffi::export]
impl EpubBook {
    /// The paths of the spine items, in reading order.
    pub fn spine(&self) -> Vec<String> {
        self.0.spine().to_vec()
    }

    /// The text of the spine item at `index`, which positions count into
    /// (ADR-0022); none beyond the spine.
    pub fn text(&self, index: u32) -> Option<String> {
        self.0.text(index as usize)
    }

    /// The length of each spine item's text in UTF-16 units, in spine order:
    /// the counts positions are measured in (ADR-0022), 0 for an unreadable item.
    /// Known from the Import, so asking for them parses no spine item.
    pub fn text_lengths(&self) -> Vec<u64> {
        self.0.characters()
    }

    /// The Table of Contents: Chapters with their nested Sections.
    pub fn table_of_contents(&self) -> Arc<TableOfContents> {
        Arc::new(TableOfContents(Arc::new(contents::TableOfContents::new(
            self.0.contents(),
        ))))
    }

    /// Where a link to `fragment` in the file at `path` points; none if `path`
    /// isn't a spine item.
    pub fn locate(&self, path: String, fragment: Option<String>) -> Option<Place> {
        self.0.locate(&path, fragment.as_deref())
    }

    /// Where the passage `quoted_text` of a Highlight from `start` to `end` is
    /// now: there if the text still matches, else where the text occurs nearest;
    /// none if it occurs nowhere.
    pub fn relocate(
        &self,
        start: EpubPosition,
        end: EpubPosition,
        quoted_text: String,
    ) -> Option<Anchor> {
        let (start, end) = self.0.relocate(start, end, &quoted_text)?;
        Some(Anchor::Epub { start, end })
    }

    /// The file at `path` (a spine item or what it uses), if the EPUB has it.
    pub fn resource(&self, path: String) -> Option<EpubResource> {
        let resource = self.0.resource(&path)?;
        Some(EpubResource {
            bytes: resource.bytes,
            media_type: resource.media_type,
        })
    }
}

/// A Table of Contents, see `pkm_books::contents::TableOfContents`. Its entries
/// come from an EPUB (`EpubBook.table_of_contents`) or, for a PDF, from its outline.
#[derive(uniffi::Object)]
pub struct TableOfContents(Arc<contents::TableOfContents>);

#[uniffi::export]
impl TableOfContents {
    /// The Table of Contents of `entries`, the Chapters with their Sections.
    #[uniffi::constructor]
    pub fn new(entries: Vec<ContentsEntry>) -> Arc<Self> {
        Arc::new(Self(Arc::new(contents::TableOfContents::new(entries))))
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The Chapters, with their Sections nested, in reading order.
    pub fn chapters(&self) -> Vec<Entry> {
        self.0.chapters()
    }

    /// Where `place` is; none before the first entry with a place.
    pub fn position(&self, place: Place) -> Option<Position> {
        self.0.position(place)
    }

    /// The title of the Chapter `place` is in, as Back names it; none before the
    /// first entry with a place.
    pub fn chapter_title(&self, place: Place) -> Option<String> {
        self.0.chapter_title(place).map(str::to_owned)
    }
}

/// Where Back returns to and where a Book Search session started, for one
/// book view, see `pkm_books::jump::JumpHistory`. Behind a lock, so each
/// call is cheap enough for every scroll, and the observer is told about Back
/// place changes outside it.
#[derive(uniffi::Object)]
pub struct JumpHistory {
    history: Mutex<jump::JumpHistory>,
    observer: Arc<dyn BackObserver>,
}

/// Told when the FFI JumpHistory's Back place changes. Implemented by the
/// client; called from the thread that changed it, which is the main thread
/// for the book views.
#[uniffi::export(with_foreign)]
pub trait BackObserver: Send + Sync {
    /// The Back place now; none when it went.
    fn back_place_changed(&self, place: Option<Place>);
}

#[uniffi::export]
impl JumpHistory {
    #[uniffi::constructor]
    pub fn new(observer: Arc<dyn BackObserver>) -> Arc<Self> {
        Arc::new(Self {
            history: Mutex::default(),
            observer,
        })
    }

    /// The top of the viewport before the latest Jump; none before any and after Back.
    pub fn back_place(&self) -> Option<Place> {
        self.history().back_place()
    }

    /// Jump: remembers `from`, the top of the viewport, for Back. Returns false,
    /// changing nothing, if there is no top or `place` is already at it.
    pub fn jump(&self, place: Place, from: Option<Place>) -> bool {
        self.change(|h| h.jump(place, from))
    }

    /// Back: the Back place, which is forgotten; none without one.
    pub fn go_back(&self) -> Option<Place> {
        self.change(|h| h.go_back())
    }

    /// Starts a Book Search session at `at`, the top of the viewport.
    pub fn begin_search_session(&self, at: Option<Place>) {
        self.change(|h| h.begin_search_session(at))
    }

    pub fn end_search_session(&self) {
        self.change(|h| h.end_search_session())
    }

    /// A Book Search match was revealed by moving the viewport.
    pub fn did_reveal(&self) {
        self.change(|h| h.did_reveal())
    }

    /// The viewport moved to `to`, the top of it.
    pub fn did_scroll(&self, to: Place) {
        self.change(|h| h.did_scroll(to))
    }
}

impl JumpHistory {
    fn history(&self) -> std::sync::MutexGuard<'_, jump::JumpHistory> {
        self.history.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Applies `change` under the lock and tells the observer when the Back
    /// place moved, after releasing it.
    fn change<T>(&self, change: impl FnOnce(&mut jump::JumpHistory) -> T) -> T {
        let (result, was, is) = {
            let mut history = self.history();
            let was = history.back_place();
            let result = change(&mut history);
            (result, was, history.back_place())
        };
        if is != was {
            self.observer.back_place_changed(is);
        }
        result
    }
}

/// The Library as `Library::listing` lists it: the Books, newest Import
/// first, and their `Listing` in the order of a `Sort`. One call makes both,
/// so they can't disagree.
#[derive(uniffi::Record)]
pub struct ListedLibrary {
    /// The Books, newest Import first.
    pub books: Vec<Book>,
    /// Library Search over `books`: positions in them, in Sort order.
    pub listing: Arc<Listing>,
}

/// What Library Search finds in the Books a `Library::listing` returned:
/// positions in them, see `pkm_books::listing::Listing`.
#[derive(uniffi::Object)]
pub struct Listing(listing::Listing);

#[uniffi::export]
impl Listing {
    /// Positions in the Books returned with this Listing of those matching
    /// `search`, in the Listing's order.
    pub fn matching(&self, search: &str) -> Vec<u32> {
        self.0
            .matching(search)
            .into_iter()
            .map(|i| i as u32)
            .collect()
    }
}

// `Book` is hand-made (computed Title and percent read): the core's
// crosses as that.
uniffi::custom_type!(CoreBook, Book, {
    remote,
    lower: |d| d.into(),
    try_lift: |_book| Err(uniffi::deps::anyhow::anyhow!("Books only go to the client")),
});

#[uniffi::remote(Enum)]
pub enum ImportResult {
    Imported(CoreBook),
    AlreadyImported(CoreBook),
}

/// Receives the progress of an Import. Implemented by the client and called
/// from the thread that runs the Import, not the main thread.
#[uniffi::export(with_foreign)]
pub trait ImportProgress: Send + Sync {
    /// `copied` bytes of the File's `total` are processed.
    fn report(&self, copied: u64, total: u64);
}

/// Why a `PdfReader` refuses a PDF, see `pkm_books::library::PdfProblem`.
#[uniffi::remote(Error)]
pub enum PdfProblem {
    Unreadable,
    Locked,
}

/// What a PDF says about itself, as a `PdfReader` reads it.
#[derive(uniffi::Record)]
pub struct PdfDetails {
    pub title: Option<String>,
    pub author: Option<String>,
    pub page_count: u32,
}

/// Reads what a PDF says about itself, with the client's PDF engine (ADR-0003).
/// Implemented by the client and called by the core from the thread of the
/// Import or of the details pass, not the main thread.
#[uniffi::export(with_foreign)]
pub trait PdfReader: Send + Sync {
    /// The details of the PDF at `path`, or why it is refused.
    fn read(&self, path: String) -> Result<PdfDetails, PdfProblem>;
}

/// Told when the Gallery should list the Library again. Implemented by the
/// client and called from the details pass's thread, not the main thread.
#[uniffi::export(with_foreign)]
pub trait GalleryObserver: Send + Sync {
    /// The pass read details: the Gallery should list the Library again.
    fn gallery_changed(&self);
}

/// Adapts the client's reader to the core's.
struct Reader(Arc<dyn PdfReader>);

impl pkm_books::library::PdfReader for Reader {
    fn read(&self, path: &Path) -> Result<Details, PdfProblem> {
        let pdf = self.0.read(path.to_string_lossy().into_owned())?;
        Ok(Details {
            title: pdf.title,
            author: pdf.author,
            size: Size::Pages(pdf.page_count),
        })
    }
}

/// Adapts the client's observer to the core's.
struct Gallery(Arc<dyn GalleryObserver>);

impl pkm_books::library::GalleryObserver for Gallery {
    fn gallery_changed(&self) {
        self.0.gallery_changed()
    }
}

/// The Library folder, see `pkm_books::library::Library`.
#[derive(uniffi::Object)]
pub struct Library {
    core: Arc<pkm_books::library::Library>,
    /// The pass that reads missing details on a background thread, see
    /// `read_missing_details`.
    details_pass: Mutex<Option<DetailsPass>>,
}

#[uniffi::export]
impl Library {
    #[uniffi::constructor]
    pub fn open(root: String) -> Result<Arc<Self>, LibraryError> {
        Ok(Arc::new(Self {
            core: Arc::new(pkm_books::library::Library::open(root)?),
            details_pass: Mutex::new(None),
        }))
    }

    /// Imports the File at `source`, reading a PDF with `reader`, see
    /// `pkm_books::library::Library::import`.
    pub fn import(
        &self,
        source: String,
        reader: Arc<dyn PdfReader>,
        progress: Arc<dyn ImportProgress>,
    ) -> Result<ImportResult, LibraryError> {
        let result = self
            .core
            .import(source.as_ref(), &Reader(reader), |copied, total| {
                progress.report(copied, total)
            });
        Ok(result?)
    }

    /// Reads the details of the Books that lack them on a background thread,
    /// one at a time (PDFs with `reader`), and tells `observer` when the
    /// Gallery should list again; the core coalesces the callbacks. A File that
    /// can't be read is retried by a later pass. Details are kept on this
    /// Device only.
    pub fn read_missing_details(
        &self,
        reader: Arc<dyn PdfReader>,
        observer: Arc<dyn GalleryObserver>,
    ) {
        *self
            .details_pass
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(DetailsPass::start(
            self.core.clone(),
            Arc::new(Reader(reader)),
            Arc::new(Gallery(observer)),
        ));
    }

    pub fn delete(&self, id: String) -> Result<(), LibraryError> {
        Ok(self.core.delete(&id)?)
    }

    /// EPUB `id`, opened for reading; none for a PDF, or an EPUB that can't be read.
    pub fn open_epub(&self, id: String) -> Result<Option<Arc<EpubBook>>, LibraryError> {
        Ok(self
            .core
            .open_epub(&id)?
            .map(|epub| Arc::new(EpubBook(Arc::new(epub)))))
    }

    /// The bytes of the Cover image of EPUB `id`, if it has one.
    pub fn epub_cover(&self, id: String) -> Result<Option<Vec<u8>>, LibraryError> {
        Ok(self.core.epub_cover(&id)?)
    }

    /// Stores `progress` as the Reading Progress of Book `id`, and says
    /// whether that changes what the Gallery for `sort` shows, see
    /// `pkm_books::library::Library::save_progress`.
    pub fn save_progress(
        &self,
        id: String,
        progress: ReadingProgress,
        sort: Sort,
    ) -> Result<bool, LibraryError> {
        Ok(self.core.save_progress(&id, &progress, sort)?)
    }

    pub fn markup(&self, id: String) -> Result<Markup, LibraryError> {
        Ok(self.core.markup(&id)?)
    }

    pub fn add_highlight(
        &self,
        id: String,
        anchor: Anchor,
        quoted_text: String,
        color: Rgb,
        created: u64,
    ) -> Result<Option<Highlight>, LibraryError> {
        Ok(self
            .core
            .add_highlight(&id, anchor, &quoted_text, color, created)?)
    }

    pub fn set_highlight_color(
        &self,
        id: String,
        highlight: String,
        color: Rgb,
    ) -> Result<(), LibraryError> {
        Ok(self.core.set_highlight_color(&id, &highlight, color)?)
    }

    pub fn set_highlight_comment(
        &self,
        id: String,
        highlight: String,
        comment: String,
    ) -> Result<(), LibraryError> {
        Ok(self.core.set_highlight_comment(&id, &highlight, &comment)?)
    }

    pub fn remove_highlight(&self, id: String, highlight: String) -> Result<(), LibraryError> {
        Ok(self.core.remove_highlight(&id, &highlight)?)
    }

    pub fn add_page_comment(
        &self,
        id: String,
        place: Place,
        created: u64,
    ) -> Result<Option<PageComment>, LibraryError> {
        Ok(self.core.add_page_comment(&id, place, created)?)
    }

    pub fn set_page_comment_text(
        &self,
        id: String,
        comment: String,
        text: String,
    ) -> Result<(), LibraryError> {
        Ok(self.core.set_page_comment_text(&id, &comment, &text)?)
    }

    pub fn remove_page_comment(&self, id: String, comment: String) -> Result<(), LibraryError> {
        Ok(self.core.remove_page_comment(&id, &comment)?)
    }

    pub fn highlight_colors(&self) -> HighlightColors {
        self.core.highlight_colors()
    }

    pub fn set_default_highlight_color(&self, id: String) -> Result<(), LibraryError> {
        Ok(self.core.set_default_highlight_color(&id)?)
    }

    pub fn add_custom_color(&self, rgb: Rgb, added: u64) -> Result<HighlightColor, LibraryError> {
        Ok(self.core.add_custom_color(rgb, added)?)
    }

    pub fn remove_custom_color(&self, id: String) -> Result<(), LibraryError> {
        Ok(self.core.remove_custom_color(&id)?)
    }

    /// The Books, newest Import first, and what Library Search finds in
    /// them in the order of `sort`. One call makes both, so they can't disagree.
    pub fn listing(&self, sort: Sort) -> Result<ListedLibrary, LibraryError> {
        let (books, listing) = self.core.listing(sort)?;
        Ok(ListedLibrary {
            books: books.into_iter().map(Into::into).collect(),
            listing: Arc::new(Listing(listing)),
        })
    }

    pub fn book(&self, id: String) -> Result<Option<Book>, LibraryError> {
        Ok(self.core.book(&id)?.map(Into::into))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
    use std::time::Duration;

    #[test]
    fn a_book_text_is_searched_chunk_by_chunk_through_the_ffi() {
        /// Reads each page's text once, and remembers how often it was asked.
        struct Pages(AtomicUsize);
        impl PdfPages for Pages {
            fn page_text(&self, _: u32) -> Option<String> {
                self.0.fetch_add(1, Relaxed);
                Some("apple".to_owned())
            }
            fn page_number(&self, index: u32) -> String {
                (index + 1).to_string()
            }
            fn place_of_match(&self, index: u32, _: u64, _: u64) -> Option<Place> {
                Some(Place::Pdf {
                    page_index: index,
                    x: 0.0,
                    y: 720.0,
                })
            }
        }

        let text = BookText::pdf(2, Arc::new(Pages(AtomicUsize::new(0))));
        let contents = TableOfContents::new(vec![]);
        let session = text.search(
            Query {
                text: "apple".to_owned(),
                case_sensitive: false,
                word_match: WordMatch::Contains,
            },
            contents.clone(),
        );

        let first = session.next().unwrap();
        assert_eq!(first.len(), 1);
        assert_eq!((first[0].chunk, first[0].start, first[0].end), (0, 0, 5));
        assert_eq!(first[0].text_start, 0);
        assert_eq!(session.next().unwrap().len(), 1);
        assert!(session.next().is_none());

        // Each page was read once for both chunks' searches together.
        assert_eq!(text.chunk(1).unwrap().length, 5);
        assert_eq!(text.chunk_containing(5).unwrap().index, 1);
        assert_eq!(session.number_at(5), Some(1));
    }

    /// The Back places that the FFI JumpHistory reports through `BackObserver`.
    #[derive(Default)]
    struct BackPlaces(Mutex<Vec<Option<Place>>>);

    impl BackObserver for BackPlaces {
        fn back_place_changed(&self, place: Option<Place>) {
            self.0.lock().unwrap().push(place);
        }
    }

    impl BackPlaces {
        fn places(&self) -> Vec<Option<Place>> {
            self.0.lock().unwrap().clone()
        }
    }

    fn pdf(page_index: u32, y: f64) -> Place {
        Place::Pdf {
            page_index,
            x: 0.0,
            y,
        }
    }

    #[test]
    fn a_jump_reports_the_back_place_when_it_appears_and_back_when_it_goes() {
        let places = Arc::new(BackPlaces::default());
        let history = JumpHistory::new(places.clone());
        // A Jump that changes nothing reports nothing.
        history.jump(pdf(10, 700.0), None);
        history.jump(pdf(10, 700.0), Some(pdf(10, 700.0)));
        assert!(places.places().is_empty());

        history.jump(pdf(10, 700.0), Some(pdf(1, 700.0)));
        assert_eq!(places.places(), [Some(pdf(1, 700.0))]);
        assert_eq!(history.go_back(), Some(pdf(1, 700.0)));
        assert_eq!(places.places(), [Some(pdf(1, 700.0)), None]);
        // A Back without a Back place reports nothing.
        history.go_back();
        assert_eq!(places.places().len(), 2);
    }

    #[test]
    fn a_scroll_that_ends_back_reports_it_going() {
        let places = Arc::new(BackPlaces::default());
        let history = JumpHistory::new(places.clone());
        history.jump(pdf(10, 700.0), Some(pdf(1, 700.0)));
        history.did_scroll(pdf(11, 700.0));
        assert_eq!(places.places(), [Some(pdf(1, 700.0))]);
        history.did_scroll(pdf(12, 700.0));
        assert_eq!(places.places(), [Some(pdf(1, 700.0)), None]);
    }

    #[test]
    fn a_search_session_reports_its_back_place_once() {
        let places = Arc::new(BackPlaces::default());
        let history = JumpHistory::new(places.clone());
        history.begin_search_session(Some(pdf(1, 700.0)));
        history.did_reveal();
        assert_eq!(places.places(), [Some(pdf(1, 700.0))]);
        // Reveals, scrolls and ending the session leave the Back place alone.
        history.did_reveal();
        history.did_scroll(pdf(20, 700.0));
        history.end_search_session();
        assert_eq!(places.places().len(), 1);
    }

    /// Counts the Sync steps that ended.

    #[test]
    fn the_library_lists_its_books_by_id_in_order_and_searches_them() {
        let dir =
            std::env::temp_dir().join(format!("pkm-books-ffi-listing-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let library = Library::open(dir.join("library").display().to_string()).unwrap();
        let mut ids = vec![];
        let books = [
            ("Zebra", None),
            ("apple 10", Some("Émile Zola")),
            ("apple 2", None),
        ];
        for (n, (title, author)) in books.into_iter().enumerate() {
            let source = dir.join(format!("{n}.pdf"));
            std::fs::write(&source, format!("%PDF-{n}")).unwrap();
            let path = source.display().to_string();
            let reader = Reads {
                title: Some(title.to_owned()),
                author: author.map(str::to_owned),
            };
            let Ok(ImportResult::Imported(book)) =
                library.import(path, Arc::new(reader), Arc::new(NoProgress))
            else {
                panic!("import")
            };
            ids.push(book.id);
            std::thread::sleep(Duration::from_millis(2));
        }
        let listed = |key, reversed, search: &str| {
            let listed = library.listing(Sort { key, reversed }).unwrap();
            let books = listed.books;
            let matched = listed.listing.matching(search);
            matched
                .into_iter()
                .map(|i| books[i as usize].id.clone())
                .collect::<Vec<_>>()
        };
        let [zebra, ten, two] = [&ids[0], &ids[1], &ids[2]].map(String::as_str);
        assert_eq!(listed(SortKey::DateImported, false, ""), [two, ten, zebra]);
        assert_eq!(listed(SortKey::Title, false, ""), [two, ten, zebra]);
        assert_eq!(listed(SortKey::Title, true, ""), [zebra, ten, two]);
        assert_eq!(listed(SortKey::Title, false, "emile"), [ten]);
        assert_eq!(listed(SortKey::Title, false, "2.PDF"), [two]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// The Library's Books, in the order the Gallery lists them by default.
    fn listed_books(library: &Library) -> Vec<Book> {
        let sort = Sort {
            key: SortKey::DateImported,
            reversed: false,
        };
        library.listing(sort).unwrap().books
    }

    struct NoProgress;

    impl ImportProgress for NoProgress {
        fn report(&self, _: u64, _: u64) {}
    }

    /// Reads every PDF as one page, with `title` and `author`.
    #[derive(Default)]
    struct Reads {
        title: Option<String>,
        author: Option<String>,
    }

    impl PdfReader for Reads {
        fn read(&self, _: String) -> Result<PdfDetails, PdfProblem> {
            Ok(PdfDetails {
                title: self.title.clone(),
                author: self.author.clone(),
                page_count: 1,
            })
        }
    }

    /// Refuses every PDF.
    struct Refuses(PdfProblem);

    impl PdfReader for Refuses {
        fn read(&self, _: String) -> Result<PdfDetails, PdfProblem> {
            Err(self.0)
        }
    }

    #[test]
    fn import_refuses_a_file_that_is_neither_a_pdf_nor_an_epub() {
        let dir = std::env::temp_dir().join(format!("pkm-books-ffi-format-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("notes.txt");
        std::fs::write(&source, b"hello").unwrap();
        let path = source.display().to_string();
        let library = Library::open(dir.join("library").display().to_string()).unwrap();

        let refused = library.import(path, Arc::new(Reads::default()), Arc::new(NoProgress));
        assert!(matches!(refused, Err(LibraryError::UnsupportedFormat)));
        assert_eq!(listed_books(&library).len(), 0);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_pdf_the_reader_refuses_is_refused_with_its_reason() {
        let dir =
            std::env::temp_dir().join(format!("pkm-books-ffi-refused-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("locked.pdf");
        std::fs::write(&source, b"%PDF-1.7 locked").unwrap();
        let path = source.display().to_string();
        let library = Library::open(dir.join("library").display().to_string()).unwrap();

        let refused = library.import(
            path,
            Arc::new(Refuses(PdfProblem::Locked)),
            Arc::new(NoProgress),
        );
        assert!(matches!(refused, Err(LibraryError::LockedPdf)));
        assert_eq!(listed_books(&library).len(), 0);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
