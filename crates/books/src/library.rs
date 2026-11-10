//! The Library: the Books a user has imported.
//!
//! Layout: see `store`. Files sit in the Books folder under their own names,
//! the state of each Book hidden beside them, found by file name, else by
//! SHA-256. A Book's ID is its Item ID, random at Import. Imports are staged
//! and renamed into place, so a Book is either fully there or not at all.
//!
//! The Books are found when the Library is opened and by each listing
//! (`books`), and kept in memory: Imports, Deletes and edits change that and
//! the files together.
//!
//! A `DetailsPass` reads in the background the details Books lack, one Book
//! at a time, telling the Gallery when to list again.

use crate::book_cache::BookCache;
pub use crate::details::format_of;
use crate::details::{self, Details, Format, Size};
use crate::epub::{self, Epub};
use crate::files::{self, flush_ordered, unique_name, write_atomically};
use crate::listing::{Listing, Sort, SortKey, progress_changes_gallery};
use crate::markup::{Anchor, Highlight, Markup, PageComment, Rgb};
use crate::preferences::{HighlightColor, HighlightColors, Preferences};
use crate::progress::{Place, ReadingProgress};
use crate::state::{self, BookState};
use crate::store::Folder;
use pkm_base::item_id::ItemId;
use pkm_base::sha256::Sha256;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime};

/// The fewest Books `books` lists on a thread of their own.
const MIN_PER_THREAD: usize = 32;

/// A Book in the Library.
#[derive(Debug, Clone, PartialEq)]
pub struct Book {
    /// The Item ID.
    pub id: String,
    /// The stored File.
    pub path: PathBuf,
    /// The SHA-256 of the File, lowercase hex: what is made from its content,
    /// e.g. a Cover, is the same for the same value.
    pub sha256: String,
    /// Date Imported, in milliseconds since the Unix epoch.
    pub imported: u64,
    /// Where the user left off; none if the Book was never opened, or its
    /// state is missing or unreadable.
    pub progress: Option<ReadingProgress>,
    /// What was read from the File: at Import, or by `read_details` for a PDF
    /// read later. None until then, or if they are unreadable.
    pub details: Option<Details>,
}

impl Book {
    /// An EPUB once its details are read, else a PDF.
    pub fn format(&self) -> Format {
        self.details.as_ref().map_or(Format::Pdf, Details::format)
    }

    /// The Title: the title stored in the File, or else its name without
    /// extension.
    pub fn title(&self) -> String {
        self.details
            .as_ref()
            .and_then(|d| d.title.clone())
            .unwrap_or_else(|| {
                let stem = self.path.file_stem().unwrap_or_default();
                stem.to_string_lossy().into_owned()
            })
    }

    /// How far the user has got, in whole percent rounded down. For a PDF it
    /// counts the page they are on: 100 only on the last page. For an EPUB it
    /// counts the characters before their place: 100 only at the end. None if
    /// the Book was never opened, its size isn't known yet, or the Reading
    /// Progress is of another Format.
    pub fn percent_read(&self) -> Option<u8> {
        let (reached, total) = match (&self.details.as_ref()?.size, self.progress?.place) {
            (Size::Pages(pages), Place::Pdf { page_index, .. }) => {
                (u64::from(page_index) + 1, u64::from(*pages))
            }
            (
                Size::Characters(items),
                Place::Epub {
                    spine_index,
                    offset,
                },
            ) => {
                let before = items.iter().take(spine_index as usize);
                let in_item = items
                    .get(spine_index as usize)
                    .map_or(0, |&n| offset.min(n));
                (before.sum::<u64>() + in_item, items.iter().sum())
            }
            _ => return None,
        };
        (total > 0).then(|| (100 * u128::from(reached) / u128::from(total)).min(100) as u8)
    }
}

/// Why an Import failed.
#[derive(Debug)]
pub enum ImportError {
    /// Reading or copying the File failed. Nothing was imported.
    Io(io::Error),
    /// The File is an EPUB the app can't read. Nothing was imported.
    Epub(epub::Error),
    /// The File is neither a PDF nor an EPUB. Nothing was imported.
    UnsupportedFormat,
    /// The PDF can't be opened. Nothing was imported.
    UnreadablePdf,
    /// The PDF needs a password to open. Nothing was imported.
    LockedPdf,
}

impl From<io::Error> for ImportError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<epub::Error> for ImportError {
    fn from(e: epub::Error) -> Self {
        Self::Epub(e)
    }
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => e.fmt(f),
            Self::Epub(e) => e.fmt(f),
            Self::UnsupportedFormat => f.write_str("not a PDF or EPUB"),
            Self::UnreadablePdf => PdfProblem::Unreadable.fmt(f),
            Self::LockedPdf => PdfProblem::Locked.fmt(f),
        }
    }
}

impl std::error::Error for ImportError {}

/// Why a `PdfReader` refuses a PDF.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfProblem {
    /// It can't be opened, e.g. because it is damaged.
    Unreadable,
    /// It needs a password to open. PDFs that only restrict permissions aren't locked.
    Locked,
}

impl std::fmt::Display for PdfProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Unreadable => "the PDF can't be opened",
            Self::Locked => "the PDF is password-protected",
        })
    }
}

impl From<PdfProblem> for ImportError {
    fn from(problem: PdfProblem) -> Self {
        match problem {
            PdfProblem::Unreadable => Self::UnreadablePdf,
            PdfProblem::Locked => Self::LockedPdf,
        }
    }
}

/// Reads what a PDF says about itself: the client's PDF engine does, as the
/// core never parses PDFs (ADR-0003). The core calls it with the path of the
/// File, before the File is copied and when a Book lacks its details, from
/// the thread of the Import or of the details pass, not the main thread.
pub trait PdfReader: Send + Sync {
    /// The details of the PDF at `path`, with `Size::Pages`; or why it is refused.
    fn read(&self, path: &Path) -> Result<Details, PdfProblem>;
}

/// Told what the details pass did, from its thread, not the main thread.
pub trait GalleryObserver: Send + Sync {
    /// The pass read details: the Gallery should list the Library again.
    fn gallery_changed(&self);
}

/// The fewest details one pass reads between two calls to `GalleryObserver`,
/// so a burst of reads tells the Gallery once, not once per PDF.
const BATCH: usize = 16;
/// The longest a pass reads without telling the Gallery, so the first reads
/// show while a slow read holds up the rest of the batch.
const BATCH_SPAN: Duration = Duration::from_millis(500);

/// The background pass that reads missing details, started by
/// `DetailsPass::start`. Dropping it stops the pass once the Book it is
/// reading is done.
pub struct DetailsPass {
    cancelled: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl DetailsPass {
    /// Starts the pass on a background thread: it reads the details the
    /// Library lacks, one Book at a time (PDFs with `reader`), and tells
    /// `observer` when the Gallery should list again — coalesced, so a burst
    /// of reads lists once. A File that can't be read, e.g. a PDF `reader`
    /// refuses, is skipped, and a later pass tries it again. Details stay on
    /// this Device.
    pub fn start(
        library: Arc<Library>,
        reader: Arc<dyn PdfReader>,
        observer: Arc<dyn GalleryObserver>,
    ) -> Self {
        let cancelled = Arc::new(AtomicBool::new(false));
        let thread = thread::Builder::new()
            .name("details".into())
            .spawn({
                let cancelled = cancelled.clone();
                move || {
                    // Below the UI's priority, as work the user didn't ask for.
                    #[cfg(target_vendor = "apple")]
                    unsafe {
                        libc::pthread_set_qos_class_self_np(
                            libc::qos_class_t::QOS_CLASS_UTILITY,
                            0,
                        );
                    }
                    run_pass(&library, reader.as_ref(), observer.as_ref(), &cancelled)
                }
            })
            .expect("spawn the details pass");
        Self {
            cancelled,
            thread: Some(thread),
        }
    }
}

impl Drop for DetailsPass {
    /// Stops the thread, waiting for the Book it may be reading.
    fn drop(&mut self) {
        self.cancelled.store(true, Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// One pass: reads the Books without details, newest Import first, and tells
/// `observer` when the Gallery should list again — at most once per `BATCH`
/// details read or `BATCH_SPAN` elapsed, and once more after the last read.
/// The Gallery lists every Book again, so not one Book at a time.
fn run_pass(
    library: &Library,
    reader: &dyn PdfReader,
    observer: &dyn GalleryObserver,
    cancelled: &AtomicBool,
) {
    let mut read = 0;
    let mut told = Instant::now();
    for id in library.books_lacking_details() {
        if cancelled.load(Relaxed) {
            break;
        }
        if library.read_details(&id, reader).unwrap_or(false) {
            read += 1;
            if read >= BATCH || told.elapsed() >= BATCH_SPAN {
                observer.gallery_changed();
                read = 0;
                told = Instant::now();
            }
        }
    }
    if read > 0 {
        observer.gallery_changed();
    }
}

/// What an Import did.
#[derive(Debug, Clone, PartialEq)]
pub enum ImportResult {
    /// The File was copied into the Library as a new Book.
    Imported(Book),
    /// The Library already held a File with the same content, possibly under
    /// another name. Nothing was copied; this is the existing Book.
    AlreadyImported(Book),
}

/// The Library: the Books folder, with its Books kept in memory.
#[derive(Debug)]
pub struct Library {
    folder: Folder,
    /// The state of each Book, by Item ID. Found by `refresh` and changed by
    /// Imports, Deletes and edits, which write the files under this lock, so
    /// they never interleave.
    books: Mutex<HashMap<String, BookState>>,
    /// Held while details or the Preferences are read, changed and written, so
    /// concurrent edits never overwrite each other. Taken before `books`, if
    /// both are.
    writing: Mutex<()>,
    /// The details of each Book read so far, by ID, see `details_of`. Changed
    /// under `writing` by `set_details`, the only writer of the files.
    details: BookCache<Details>,
}

impl Library {
    /// Opens the Library in the Books folder `root`, creating it if needed and
    /// discarding leftovers of Imports that never finished. Files in it that
    /// are Books but have no state yet get one: see `Folder::scan`.
    pub fn open(root: impl Into<PathBuf>) -> io::Result<Self> {
        let folder = Folder::open(root)?;
        let books = index(folder.scan()?);
        Ok(Library {
            folder,
            books: Mutex::new(books),
            writing: Mutex::new(()),
            details: BookCache::new(),
        })
    }

    /// Finds the Books in the folder again, so Files added, renamed or deleted
    /// outside the app, and states changed by Sync, are seen.
    fn refresh(&self) -> io::Result<()> {
        self.rescan(&mut self.catalog())
    }

    /// Replaces `books` with the Books the folder holds now (`Folder::scan`),
    /// forgetting the details of those whose File changed.
    fn rescan(&self, books: &mut HashMap<String, BookState>) -> io::Result<()> {
        let found = index(self.folder.scan()?);
        for (id, state) in &found {
            if books.get(id).is_some_and(|old| old.sha256 != state.sha256) {
                self.details.forget(id);
            }
        }
        *books = found;
        Ok(())
    }

    /// Copies the File at `source` into the Library, unchanged and under its
    /// own name, or with " 2", " 3"... if that is taken. Importing a File that
    /// is already there copies nothing and returns its Book as
    /// `AlreadyImported`.
    ///
    /// A File that is neither a PDF nor an EPUB (see [`format_of`]) is
    /// rejected before anything is copied. So is an EPUB that isn't a valid
    /// reflowable one without DRM, and a PDF that `reader` refuses. The details
    /// of a PDF are read by `reader`, those of an EPUB by the core, and kept.
    ///
    /// `progress(copied, total)` is called with the bytes processed so far out
    /// of the File's size: first with 0, last with the total.
    pub fn import(
        &self,
        source: &Path,
        reader: &dyn PdfReader,
        progress: impl FnMut(u64, u64),
    ) -> Result<ImportResult, ImportError> {
        let name = source
            .file_name()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "source has no file name"))?
            .to_string_lossy()
            .into_owned();
        let format = format_of(source)?.ok_or(ImportError::UnsupportedFormat)?;
        // Read before anything is copied, so a File that is refused leaves nothing behind.
        let details = match format {
            Format::Pdf => reader.read(source)?,
            Format::Epub => read_epub_details(source)?,
        };

        let staged = self.folder.staging().join(unique_name());
        let result = self.import_staged(source, &staged, &name, progress);
        // Nothing left to remove after a successful rename.
        let _ = fs::remove_file(&staged);

        let (id, is_new) = result?;
        self.set_details(&id, &details)?;
        let book = self
            .book(&id)?
            .ok_or_else(|| io::Error::other("imported Book is empty"))?;
        Ok(if is_new {
            ImportResult::Imported(book)
        } else {
            ImportResult::AlreadyImported(book)
        })
    }

    /// Copies `source` into the `staged` file and moves it into the Library as
    /// a Book called `name`, unless the Library has the File already. Returns
    /// the Book's ID, and whether it is new.
    fn import_staged(
        &self,
        source: &Path,
        staged: &Path,
        name: &str,
        progress: impl FnMut(u64, u64),
    ) -> io::Result<(String, bool)> {
        let sha256 = copy_hashed(source, staged, progress)?;
        let mut books = self.catalog();
        // Outside changes first, so a File added there is not imported twice.
        self.rescan(&mut books)?;
        // A duplicate keeps the name it was first imported under.
        if let Some(book) = books.values().find(|book| book.sha256 == sha256) {
            return Ok((book.id.clone(), false));
        }
        let mut id = state::new_id();
        while books.contains_key(&id) {
            id = state::new_id();
        }
        let book = BookState {
            id,
            file_name: self.folder.free_name(name),
            sha256,
            imported: now(),
            markup: Markup::default(),
            progress: None,
            changes: Default::default(),
        };
        fs::rename(staged, self.folder.file(&book.file_name))?;
        self.folder.write_state(&book)?;
        let id = book.id.clone();
        books.insert(id.clone(), book);
        Ok((id, true))
    }

    /// All Books, newest Import first. Sees what changed in the folder
    /// outside the app.
    pub fn books(&self) -> io::Result<Vec<Book>> {
        self.refresh()?;
        let mut books = self.books_without_details();
        // Reading each Book's details dominates, so a large Library is spread
        // over the cores: 1,000 Books list in half the time. A small one lists
        // on this thread.
        let threads = std::thread::available_parallelism().map_or(1, usize::from);
        let per_thread = chunk_size(books.len(), threads);
        let read = |chunk: &mut [Book]| {
            for book in chunk {
                book.details = self.details_of(&book.id);
            }
        };
        if books.len() <= per_thread {
            read(&mut books);
        } else {
            std::thread::scope(|scope| {
                for chunk in books.chunks_mut(per_thread) {
                    scope.spawn(move || read(chunk));
                }
            });
        }
        Ok(books)
    }

    /// The Books, newest Import first, and their `Listing` in the order of
    /// `sort`, ready for Library Search: `matching` gives positions in the
    /// Books. One call lists both, so a client holding both sees the same
    /// Books.
    pub fn listing(&self, sort: Sort) -> io::Result<(Vec<Book>, Listing)> {
        let books = self.books()?;
        let listing = Listing::new(&books, sort);
        Ok((books, listing))
    }

    /// The Books as they are known, newest Import first, without their details.
    fn books_without_details(&self) -> Vec<Book> {
        let mut books: Vec<Book> = self
            .catalog()
            .values()
            .map(|state| self.book_of(state, None))
            .collect();
        books.sort_by(|a, b| b.imported.cmp(&a.imported).then_with(|| a.id.cmp(&b.id)));
        books
    }

    /// Book `id`, as `books` lists it; none if it isn't in the Library. An `id`
    /// that isn't an Item ID is `InvalidInput`.
    pub fn book(&self, id: &str) -> io::Result<Option<Book>> {
        check_id(id)?;
        let book = self
            .catalog()
            .get(id)
            .map(|state| self.book_of(state, None));
        Ok(book.map(|book| Book {
            details: self.details_of(id),
            ..book
        }))
    }

    /// EPUB `id`, opened for reading, with the character counts stored at Import
    /// so that they need no scan; none for a PDF, a Book that isn't in the
    /// Library, or an EPUB that can't be opened anymore. An `id` that isn't an
    /// Item ID is `InvalidInput`.
    pub fn open_epub(&self, id: &str) -> io::Result<Option<Epub>> {
        let Some(book) = self.book(id)? else {
            return Ok(None);
        };
        if book.format() != Format::Epub {
            return Ok(None);
        }
        // Empty, hence ignored by `with_characters`, for a Book without them.
        let stored = match book.details {
            Some(Details {
                size: Size::Characters(items),
                ..
            }) => items,
            _ => Vec::new(),
        };
        Ok(Epub::open(&book.path)
            .ok()
            .map(|epub| epub.with_characters(stored)))
    }

    /// The bytes of the Cover image of EPUB `id`; none for a PDF, a Book that
    /// isn't in the Library, or an EPUB without images. An `id` that isn't an
    /// Item ID is `InvalidInput`.
    pub fn epub_cover(&self, id: &str) -> io::Result<Option<Vec<u8>>> {
        Ok(self.open_epub(id)?.and_then(|epub| epub.cover()))
    }

    /// The IDs of the Books without details, newest Import first: those the
    /// details pass reads. A Book gets details at Import; it lacks them if its
    /// File arrived otherwise (put in the folder, Synced) or was edited since.
    pub(crate) fn books_lacking_details(&self) -> Vec<String> {
        self.books_without_details()
            .into_iter()
            .filter(|book| self.details_of(&book.id).is_none())
            .map(|book| book.id)
            .collect()
    }

    /// Reads the details of Book `id`, if it has none, and keeps them: a PDF's
    /// with `reader`, an EPUB's in the core. A File that can't be read keeps
    /// none. Returns whether they were kept. An `id` that isn't an Item ID is
    /// `InvalidInput`.
    pub(crate) fn read_details(&self, id: &str, reader: &dyn PdfReader) -> io::Result<bool> {
        check_id(id)?;
        let book = self
            .catalog()
            .get(id)
            .map(|state| self.book_of(state, None));
        let Some(book) = book.filter(|book| self.details_of(&book.id).is_none()) else {
            return Ok(false);
        };
        let details = match format_of(&book.path) {
            Ok(Some(Format::Pdf)) => reader.read(&book.path).ok(),
            Ok(Some(Format::Epub)) => read_epub_details(&book.path).ok(),
            _ => None,
        };
        let Some(details) = details else {
            return Ok(false);
        };
        self.set_details(id, &details)?;
        Ok(true)
    }

    /// Keeps `details`, read from the File of Book `id`, on this Device,
    /// replacing any before; `Book::details` returns them as `details` stores
    /// them. Does nothing for a Book that isn't in the Library. An `id` that
    /// isn't an Item ID is `InvalidInput`.
    fn set_details(&self, id: &str, details: &Details) -> io::Result<()> {
        check_id(id)?;
        let bytes = details::encode(details);
        // Under `writing`, so the cache ends as the last write left the file,
        // and a Delete in between leaves no details behind.
        let _writing = self.writing();
        if !self.catalog().contains_key(id) {
            return Ok(());
        }
        write_atomically(&self.folder.staging(), &self.folder.details(id), &bytes)?;
        // As stored, i.e. normalized.
        self.details.set(id, details::decode(&bytes));
        Ok(())
    }

    /// The details of Book `id`. Read once, then kept in memory.
    fn details_of(&self, id: &str) -> Option<Details> {
        self.details.get_or_read(id, || {
            fs::read(self.folder.details(id))
                .ok()
                .and_then(|bytes| details::decode(&bytes))
        })
    }

    /// The Book of `state`, with `details`.
    fn book_of(&self, state: &BookState, details: Option<Details>) -> Book {
        Book {
            id: state.id.clone(),
            path: self.folder.file(&state.file_name),
            sha256: state.sha256.clone(),
            imported: state.imported,
            progress: state.progress,
            details,
        }
    }

    /// Stores `progress` as the Reading Progress of Book `id`, replacing the
    /// previous one, without a trace, and says whether that changes what the
    /// Gallery for `sort` shows: see `listing::progress_changes_gallery`.
    /// Saving for a Book that isn't in the Library (never imported, or
    /// Deleted, even just now) does nothing and says no change. An `id` that
    /// isn't an Item ID is `InvalidInput`.
    pub fn save_progress(
        &self,
        id: &str,
        progress: &ReadingProgress,
        sort: Sort,
    ) -> io::Result<bool> {
        check_id(id)?;
        let mut books = self.catalog();
        let Some(state) = books.get(id) else {
            return Ok(false);
        };
        let before = self.book_of(state, self.details_of(id));
        let after = Book {
            progress: Some(*progress),
            ..before.clone()
        };
        let last_reads = match sort.key {
            SortKey::RecentlyRead => books
                .values()
                .filter(|other| other.id != id)
                .filter_map(|other| other.progress)
                .map(|p| p.last_read)
                .collect(),
            _ => Vec::new(),
        };
        let changes = progress_changes_gallery(sort, &before, &after, last_reads);
        self.store(&mut books, id, |state| state.progress = Some(*progress))?;
        Ok(changes)
    }

    /// The Markup of Book `id`, oldest first: empty if it has none, or the
    /// Book isn't in the Library. An `id` that isn't an Item ID is
    /// `InvalidInput`.
    pub fn markup(&self, id: &str) -> io::Result<Markup> {
        check_id(id)?;
        Ok(self
            .catalog()
            .get(id)
            .map(|state| state.markup.listed())
            .unwrap_or_default())
    }

    /// Adds a Highlight of `quoted_text` at `anchor` in `color`, `created` in
    /// milliseconds since the Unix epoch, to Book `id`, and returns it. It is
    /// independent of any Highlight already there, also on the same text. For
    /// a Book that isn't in the Library it adds nothing and returns `None`. A
    /// corrupt state is replaced.
    pub fn add_highlight(
        &self,
        id: &str,
        anchor: Anchor,
        quoted_text: &str,
        color: Rgb,
        created: u64,
    ) -> io::Result<Option<Highlight>> {
        let highlight = Highlight {
            id: state::new_id(),
            anchor,
            quoted_text: quoted_text.to_owned(),
            color,
            created,
            comment: None,
        };
        let added = self.edit_state(id, |state| state.markup.highlights.push(highlight.clone()))?;
        Ok(added.then_some(highlight))
    }

    /// Sets the color of Highlight `highlight` of Book `id`. Does nothing for a
    /// Highlight that isn't there, or a Book that isn't in the Library.
    pub fn set_highlight_color(&self, id: &str, highlight: &str, color: Rgb) -> io::Result<()> {
        self.edit_state(id, |s| s.set_highlight_color(highlight, color, now()))
            .map(drop)
    }

    /// Sets the Comment of Highlight `highlight` of Book `id` to `comment`.
    /// Empty `comment` deletes the Comment. Does nothing for a Highlight that
    /// isn't there, or a Book that isn't in the Library.
    pub fn set_highlight_comment(
        &self,
        id: &str,
        highlight: &str,
        comment: &str,
    ) -> io::Result<()> {
        self.edit_state(id, |s| s.set_highlight_comment(highlight, comment, now()))
            .map(drop)
    }

    /// Removes Highlight `highlight` from Book `id`, without a trace. Does
    /// nothing for a Highlight that isn't there, or a Book that isn't in the
    /// Library.
    pub fn remove_highlight(&self, id: &str, highlight: &str) -> io::Result<()> {
        self.edit_state(id, |s| s.remove_highlight(highlight, now()))
            .map(drop)
    }

    /// Adds a page Comment at `place`, a spot of the Format of Book `id`,
    /// `created` in milliseconds since the Unix epoch, and returns it, still
    /// without text: it isn't in the Markup until `set_page_comment_text` gives
    /// it some. Like `add_highlight`, it adds nothing and returns `None` for a
    /// Book that isn't in the Library.
    pub fn add_page_comment(
        &self,
        id: &str,
        place: Place,
        created: u64,
    ) -> io::Result<Option<PageComment>> {
        let comment = PageComment {
            id: state::new_id(),
            place,
            created,
            text: String::new(),
        };
        let added = self.edit_state(id, |s| s.markup.page_comments.push(comment.clone()))?;
        Ok(added.then_some(comment))
    }

    /// Sets the text of page Comment `comment` of Book `id` to `text`. Empty
    /// `text` deletes the page Comment. Does nothing for a page Comment that
    /// isn't there, or a Book that isn't in the Library.
    pub fn set_page_comment_text(&self, id: &str, comment: &str, text: &str) -> io::Result<()> {
        self.edit_state(id, |s| s.set_page_comment_text(comment, text, now()))
            .map(drop)
    }

    /// Removes page Comment `comment` from Book `id`, without a trace. Does
    /// nothing for a page Comment that isn't there, or a Book that isn't in
    /// the Library.
    pub fn remove_page_comment(&self, id: &str, comment: &str) -> io::Result<()> {
        self.edit_state(id, |s| s.remove_page_comment(comment, now()))
            .map(drop)
    }

    /// The Highlight Colors to choose from, the six built-in ones (yellow
    /// first) then the Custom Colors in the order added, together with the
    /// Default Highlight Color, the color of new Highlights: yellow until the
    /// user picks another, or if the Preferences are unreadable or its Custom
    /// Color was removed. Both come from one load of the Preferences.
    pub fn highlight_colors(&self) -> HighlightColors {
        self.read_preferences().highlight_colors()
    }

    /// Makes the Highlight Color `id`, a built-in color's name or a Custom
    /// Color's ID, the Default Highlight Color. An `id` that is no Highlight
    /// Color is `InvalidInput`. Corrupt Preferences are replaced.
    pub fn set_default_highlight_color(&self, id: &str) -> io::Result<()> {
        if !self.highlight_colors().colors.iter().any(|c| c.id == id) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{id:?} is not a Highlight Color"),
            ));
        }
        self.edit_preferences(|p| p.set_default_color(id))
    }

    /// Adds `rgb` as a Custom Color after the others, `added` in milliseconds
    /// since the Unix epoch, and returns it. There is no limit. Corrupt
    /// Preferences are replaced.
    pub fn add_custom_color(&self, rgb: Rgb, added: u64) -> io::Result<HighlightColor> {
        let mut color = None;
        self.edit_preferences(|p| color = Some(p.add_custom_color(rgb, added)))?;
        Ok(color.expect("the edit ran"))
    }

    /// Removes the Custom Color `id`. Highlights keep their color. If it was
    /// the Default Highlight Color, that is yellow again. Does nothing for a
    /// built-in color, which can't be removed, or an unknown `id`.
    pub fn remove_custom_color(&self, id: &str) -> io::Result<()> {
        self.edit_preferences(|p| p.remove_custom_color_at(id, now()))
    }

    fn read_preferences(&self) -> Preferences {
        Preferences::decode(fs::read(self.folder.preferences()).ok().as_deref())
    }

    fn edit_preferences(&self, change: impl FnOnce(&mut Preferences)) -> io::Result<()> {
        let _writing = self.writing();
        let mut preferences = self.read_preferences();
        change(&mut preferences);
        write_atomically(
            &self.folder.staging(),
            &self.folder.preferences(),
            &preferences.encode(),
        )
    }

    /// Makes `change` to the state of Book `id`, writing it if it changed.
    /// Returns whether the Book is in the Library, i.e. whether `change` ran.
    fn edit_state(&self, id: &str, change: impl FnOnce(&mut BookState)) -> io::Result<bool> {
        check_id(id)?;
        let mut books = self.catalog();
        self.store(&mut books, id, change)
    }

    /// Makes `change` to the state of Book `id` among `books`, and writes it
    /// if it changed; memory changes only if the write succeeds. Returns
    /// whether the Book is there.
    fn store(
        &self,
        books: &mut HashMap<String, BookState>,
        id: &str,
        change: impl FnOnce(&mut BookState),
    ) -> io::Result<bool> {
        let Some(state) = books.get_mut(id) else {
            return Ok(false);
        };
        let mut changed = state.clone();
        change(&mut changed);
        if changed != *state {
            self.folder.write_state(&changed)?;
            *state = changed;
        }
        Ok(true)
    }

    fn writing(&self) -> MutexGuard<'_, ()> {
        self.writing.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The Books, locked.
    fn catalog(&self) -> MutexGuard<'_, HashMap<String, BookState>> {
        self.books.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Deletes the Book `id` and its File, state and details. Deleting a Book
    /// that isn't there does nothing. An `id` that isn't an Item ID (e.g. a
    /// path) is `InvalidInput`.
    ///
    /// The File goes first, so a crash mid-delete leaves a state without a
    /// File, which is a deleted Book: `open` finishes the job.
    pub fn delete(&self, id: &str) -> io::Result<()> {
        check_id(id)?;
        let _writing = self.writing();
        let mut books = self.catalog();
        let Some(state) = books.remove(id) else {
            return Ok(());
        };
        self.details.forget(id);
        self.folder.remove(&state)
    }
}

/// The states of Books by Item ID.
fn index(states: Vec<BookState>) -> HashMap<String, BookState> {
    states.into_iter().map(|s| (s.id.clone(), s)).collect()
}

/// `id` itself, if it is an Item ID. Anything else, e.g. a path, is
/// `InvalidInput`, so an ID can't point outside the Books folder.
fn check_id(id: &str) -> io::Result<()> {
    match ItemId::parse(id) {
        Some(_) => Ok(()),
        None => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{id:?} is not a Book ID"),
        )),
    }
}

/// Books per thread when listing `len` of them on up to `threads`: a thread
/// isn't worth starting for fewer than `MIN_PER_THREAD`.
fn chunk_size(len: usize, threads: usize) -> usize {
    len.div_ceil(threads).max(MIN_PER_THREAD)
}

/// The details of the EPUB `file`.
fn read_epub_details(file: &Path) -> Result<Details, epub::Error> {
    let epub = Epub::open(file)?;
    Ok(Details {
        title: epub.title().map(str::to_owned),
        author: epub.author().map(str::to_owned),
        size: Size::Characters(epub.characters()),
    })
}

/// Milliseconds since the Unix epoch.
pub(crate) fn now() -> u64 {
    files::millis(SystemTime::now())
}

/// Streams the file `from` into a new file at `to`, returning the SHA-256 of
/// the bytes.
fn copy_hashed(from: &Path, to: &Path, progress: impl FnMut(u64, u64)) -> io::Result<String> {
    let mut input = File::open(from)?;
    let total = input.metadata()?.len();
    stream_hashed(&mut input, to, total, progress)
}

/// Streams `input`, `total` bytes long, into a new file at `to`, returning the
/// SHA-256 of the bytes. `progress` is called as in `Library::import`.
fn stream_hashed(
    input: &mut dyn Read,
    to: &Path,
    total: u64,
    mut progress: impl FnMut(u64, u64),
) -> io::Result<String> {
    let mut copied = 0;
    progress(copied, total);
    let mut output = File::create_new(to)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 256 * 1024];
    loop {
        let n = match input.read(&mut buffer) {
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
        output.write_all(&buffer[..n])?;
        copied += n as u64;
        progress(copied, total);
    }
    flush_ordered(&output)?;
    Ok(hasher.finalize_hex())
}

/// Test doubles for `PdfReader` and `GalleryObserver`, and what tests of the
/// details pass wait on.
#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;
    use std::sync::Condvar;

    /// The Gallery's by-date order, for saves that don't care what the Gallery
    /// shows.
    pub(crate) fn by_date() -> Sort {
        Sort {
            key: SortKey::DateImported,
            reversed: false,
        }
    }

    /// Counts what a background thread does, and wakes tests waiting on it, so
    /// they wait on the event, not on a timer (CODING_STANDARDS.md).
    #[derive(Clone, Default)]
    pub(crate) struct Signal(Arc<(Mutex<usize>, Condvar)>);

    impl Signal {
        /// Counts one event, and wakes the waiting tests.
        fn raise(&self) -> usize {
            let (count, raised) = &*self.0;
            let mut count = count.lock().unwrap();
            *count += 1;
            raised.notify_all();
            *count
        }

        /// How many events there were.
        pub(crate) fn count(&self) -> usize {
            *self.0.0.lock().unwrap()
        }

        /// Returns once `condition` holds, checking it again after each event;
        /// fails if it doesn't within 5 s.
        pub(crate) fn wait_until(&self, condition: impl Fn() -> bool) {
            let (count, raised) = &*self.0;
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                // Read before `condition`, so an event while it runs isn't missed.
                let seen = *count.lock().unwrap();
                if condition() {
                    return;
                }
                let left = deadline.saturating_duration_since(Instant::now());
                assert!(!left.is_zero(), "condition never became true");
                let _ = raised
                    .wait_timeout_while(count.lock().unwrap(), left, |count| *count == seen)
                    .unwrap();
            }
        }
    }

    /// Counts the calls to `gallery_changed`.
    #[derive(Clone, Default)]
    pub(crate) struct Gallery(pub(crate) Signal);

    impl GalleryObserver for Gallery {
        fn gallery_changed(&self) {
            self.0.raise();
        }
    }

    /// A reader that refuses the first read and then reads every PDF as
    /// `details`, the way a pass finds a locked PDF that a later one opens.
    /// Its `Signal` counts the reads.
    #[derive(Clone)]
    pub(crate) struct RefusesOnce(pub(crate) Signal, Details);

    impl RefusesOnce {
        pub(crate) fn new(details: Details) -> Self {
            Self(Signal::default(), details)
        }
    }

    impl PdfReader for RefusesOnce {
        fn read(&self, _: &Path) -> Result<Details, PdfProblem> {
            if self.0.raise() == 1 {
                return Err(PdfProblem::Locked);
            }
            Ok(self.1.clone())
        }
    }

    /// A reader that reads every PDF as one page without a title. Its `Signal`
    /// counts the reads.
    #[derive(Clone, Default)]
    pub(crate) struct Reads(pub(crate) Signal);

    impl PdfReader for Reads {
        fn read(&self, path: &Path) -> Result<Details, PdfProblem> {
            self.0.raise();
            one_page(path)
        }
    }

    /// Reads every PDF as one page, with neither title nor author.
    pub(crate) fn one_page(_: &Path) -> Result<Details, PdfProblem> {
        Ok(Details {
            title: None,
            author: None,
            size: Size::Pages(1),
        })
    }

    /// A closure is a reader: it reads each PDF as the closure says.
    impl<F: Fn(&Path) -> Result<Details, PdfProblem> + Send + Sync> PdfReader for F {
        fn read(&self, path: &Path) -> Result<Details, PdfProblem> {
            self(path)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::{Gallery, Reads, RefusesOnce, by_date, one_page};
    use super::*;
    use crate::markup::{EpubPosition, PageRects, Rect};
    use std::fs;
    use std::sync::Arc;
    use std::thread::sleep;
    use std::time::Duration;

    /// A fresh folder under the system temp dir, removed on drop.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Self {
            use std::sync::atomic::{AtomicU32, Ordering::Relaxed};
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let dir = std::env::temp_dir().join(format!(
                "pkm-books-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Relaxed)
            ));
            fs::create_dir_all(&dir).unwrap();
            Scratch(dir)
        }

        fn file(&self, name: &str, bytes: &[u8]) -> PathBuf {
            let path = self.0.join(name);
            fs::write(&path, bytes).unwrap();
            path
        }

        fn library(&self) -> Library {
            Library::open(self.0.join("library")).unwrap()
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn import(library: &Library, source: &Path) -> Result<ImportResult, ImportError> {
        library.import(source, &one_page, |_, _| {})
    }

    /// Imports a File that is expected to be new, reading its PDF with `reader`.
    fn import_with(library: &Library, source: &Path, reader: &dyn PdfReader) -> Book {
        match library.import(source, reader, |_, _| {}).unwrap() {
            ImportResult::Imported(book) => book,
            other => panic!("expected Imported, got {other:?}"),
        }
    }

    /// Imports a File that is expected to be new.
    fn import_new(library: &Library, source: &Path) -> Book {
        import_with(library, source, &one_page)
    }

    /// Returns once the clock has moved on from `now()`, so the next Import is
    /// newer than the last.
    fn a_millisecond_later() {
        let start = now();
        while now() == start {
            std::hint::spin_loop();
        }
    }

    /// A reader that reads every PDF as `details`.
    fn reading(details: Details) -> impl PdfReader {
        move |_: &Path| -> Result<Details, PdfProblem> { Ok(details.clone()) }
    }

    /// A reader that refuses every PDF for `problem`.
    fn refusing(problem: PdfProblem) -> impl PdfReader {
        move |_: &Path| -> Result<Details, PdfProblem> { Err(problem) }
    }

    /// How many entries of `folder` aren't hidden: of a Library, its Books.
    fn visible_entries(folder: &Path) -> usize {
        fs::read_dir(folder)
            .unwrap()
            .filter(|e| {
                !e.as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with('.')
            })
            .count()
    }

    /// `library` reopened without the details of the Books `ids`, as for a Book
    /// copied in from elsewhere: details stay on the Device.
    fn forget_details(scratch: &Scratch, library: Library, ids: &[&String]) -> Library {
        drop(library);
        for id in ids {
            let _ = fs::remove_file(scratch.0.join("library/.details").join(id));
        }
        scratch.library()
    }

    /// The bytes of a File that is a PDF.
    const PDF: &[u8] = b"%PDF-abc";
    // Its SHA-256.
    const PDF_SHA256: &str = "1e8a7587529cc956d46dea481f38c623395a879a1179c277f476a9cdb88a540d";
    /// The Item ID of no Book.
    const NO_BOOK: &str = "zzzzzzzzzzzz";

    #[test]
    fn import_stores_a_byte_identical_file_under_its_original_name_and_a_state_beside_it() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let source = scratch.file("Paper.pdf", PDF);

        let book = import_new(&library, &source);

        assert!(ItemId::parse(&book.id).is_some());
        assert_eq!(book.path, scratch.0.join("library/Paper.pdf"));
        assert_eq!(fs::read(&book.path).unwrap(), PDF);
        assert_eq!(fs::read(&source).unwrap(), PDF);
        let state = fs::read(library.folder.state_file(&book.id)).unwrap();
        let state = BookState::decode(&state).unwrap();
        assert_eq!(state.sha256, PDF_SHA256);
        assert_eq!(state.file_name, "Paper.pdf");
        assert_eq!(state.imported, book.imported);
    }

    #[test]
    fn import_reports_bytes_copied_until_it_reaches_the_total() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let size = 600 * 1024;
        let mut bytes = vec![7; size];
        bytes[..5].copy_from_slice(b"%PDF-");
        let source = scratch.file("big.pdf", &bytes);

        let mut reports = Vec::new();
        library
            .import(&source, &one_page, |copied, total| {
                reports.push((copied, total))
            })
            .unwrap();

        let total = size as u64;
        assert!(reports.len() > 2, "expected progress in steps: {reports:?}");
        assert!(
            reports
                .iter()
                .all(|&(copied, t)| t == total && copied <= total)
        );
        assert!(reports.windows(2).all(|w| w[0].0 <= w[1].0));
        assert_eq!(reports.last(), Some(&(total, total)));
    }

    #[test]
    fn books_lists_newest_import_first() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let first = import_new(&library, &scratch.file("a.pdf", b"%PDF-first"));
        a_millisecond_later();
        let second = import_new(&library, &scratch.file("b.pdf", b"%PDF-second"));

        assert_eq!(library.books().unwrap(), vec![second, first]);
    }

    #[test]
    fn an_empty_library_has_no_books() {
        let scratch = Scratch::new();
        assert_eq!(scratch.library().books().unwrap(), vec![]);
    }

    use crate::epub::fixtures::Fixture;

    fn emma() -> Fixture {
        Fixture::new("<dc:title>Emma</dc:title><dc:creator>Jane Austen</dc:creator>")
            .chapters(&["<p>Hello</p>", "<p>world</p>"])
    }

    #[test]
    fn a_files_id_is_its_sha256_whatever_its_size_and_format() {
        let scratch = Scratch::new();
        // The first is several read buffers long; the second is an EPUB.
        let cases = [
            (
                vec![7; 600 * 1024],
                "2e43196f0b263f8ad3a912056cc166a1a44d49baabca1e17b21034bd6202345f",
            ),
            (
                emma().bytes(),
                "31bd61dce2a109ce47ebc71e22bed63375d09269899d7b643cd032efed58ace0",
            ),
        ];
        for (i, (bytes, sha256)) in cases.iter().enumerate() {
            let source = scratch.file(&format!("{i}"), bytes);
            let copy = scratch.0.join(format!("copy{i}"));
            assert_eq!(copy_hashed(&source, &copy, |_, _| {}).unwrap(), *sha256);
            assert_eq!(fs::read(copy).unwrap(), *bytes);
        }
    }

    #[test]
    fn an_imported_epub_is_a_book_with_format_title_author_and_size() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let bytes = emma().bytes();

        let book = import_new(&library, &scratch.file("book.epub", &bytes));

        assert_eq!(book.format(), Format::Epub);
        assert_eq!(book.title(), "Emma");
        let details = book.details.as_ref().unwrap();
        assert_eq!(details.author.as_deref(), Some("Jane Austen"));
        assert_eq!(details.size, Size::Characters(vec![5, 5]));
        // Listed that way too, and the File stays as it was.
        assert_eq!(library.books().unwrap(), std::slice::from_ref(&book));
        assert_eq!(fs::read(&book.path).unwrap(), bytes);
        assert_eq!(book.path.file_name().unwrap(), "book.epub");
    }

    #[test]
    fn an_epub_is_recognised_by_its_content_not_its_name() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let book = import_new(&library, &scratch.file("book", &emma().bytes()));
        assert_eq!(book.format(), Format::Epub);
    }

    #[test]
    fn a_file_that_is_neither_a_pdf_nor_an_epub_is_rejected_and_leaves_nothing() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let late_header = [vec![b' '; 1024], b"%PDF-".to_vec()].concat();
        for (name, bytes) in [
            ("notes.txt", b"hello".to_vec()),
            ("empty", Vec::new()),
            ("late.pdf", late_header),
        ] {
            let source = scratch.file(name, &bytes);
            assert_eq!(format_of(&source).unwrap(), None, "{name}");
            let rejected = import(&library, &source);
            assert!(
                matches!(rejected, Err(ImportError::UnsupportedFormat)),
                "{name}: {rejected:?}"
            );
        }

        assert_eq!(library.books().unwrap(), []);
        let stored = fs::read_dir(scratch.0.join("library")).unwrap();
        let stored = stored.filter(|e| {
            !e.as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with('.')
        });
        assert_eq!(stored.count(), 0);
    }

    #[test]
    fn a_pdf_with_leading_bytes_is_accepted() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let bytes = [vec![b' '; 1019], b"%PDF-1.7".to_vec()].concat();
        let source = scratch.file("odd.pdf", &bytes);
        assert_eq!(format_of(&source).unwrap(), Some(Format::Pdf));
        assert_eq!(import_new(&library, &source).format(), Format::Pdf);
    }

    #[test]
    fn an_epub_without_the_extension_is_accepted_by_its_content() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let source = scratch.file("download", &emma().bytes());
        assert_eq!(format_of(&source).unwrap(), Some(Format::Epub));
        assert_eq!(import_new(&library, &source).format(), Format::Epub);
    }

    #[test]
    fn an_epub_is_recognised_by_its_extension_in_any_case() {
        let scratch = Scratch::new();
        let source = scratch.file("Fixture.EPUB", b"not a zip");
        assert_eq!(format_of(&source).unwrap(), Some(Format::Epub));
    }

    #[test]
    fn an_epub_without_a_title_is_titled_by_its_file_name() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let book = Fixture::new("").bytes();
        let book = import_new(&library, &scratch.file("Walden.epub", &book));
        assert_eq!(book.title(), "Walden");
        assert_eq!(book.details.unwrap().author, None);
    }

    #[test]
    fn a_pdf_stays_a_pdf_whatever_its_name() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let book = import_new(&library, &scratch.file("paper.epub", b"%PDF-1.7 ..."));
        assert_eq!(book.format(), Format::Pdf);
        assert_eq!(library.epub_cover(&book.id).unwrap(), None);
    }

    #[test]
    fn an_epubs_cover_is_its_declared_one_else_the_first_image_else_none() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let cover = |book: Fixture, name: &str| {
            let book = import_new(&library, &scratch.file(name, &book.bytes()));
            library.epub_cover(&book.id).unwrap()
        };

        let declared = Fixture::new("")
            .manifest(
                r#"<item id="i" href="c.png" media-type="image/png" properties="cover-image"/>"#,
            )
            .chapters(&[r#"<img src="first.png"/>"#])
            .file("OEBPS/c.png", b"declared")
            .file("OEBPS/text/first.png", b"first");
        assert_eq!(cover(declared, "a.epub"), Some(b"declared".to_vec()));

        let undeclared = Fixture::new("x")
            .chapters(&[r#"<img src="first.png"/>"#])
            .file("OEBPS/text/first.png", b"first");
        assert_eq!(cover(undeclared, "b.epub"), Some(b"first".to_vec()));

        assert_eq!(cover(Fixture::new("y"), "c.epub"), None);
    }

    #[test]
    fn the_cover_of_a_missing_book_is_none_and_of_a_bad_id_an_error() {
        let scratch = Scratch::new();
        let library = scratch.library();
        assert_eq!(library.epub_cover(NO_BOOK).unwrap(), None);
        assert!(library.epub_cover("../x").is_err());
    }

    #[test]
    fn an_epub_already_imported_is_recognised_by_content() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let bytes = emma().bytes();
        let first = import_new(&library, &scratch.file("a.epub", &bytes));

        let again = import(&library, &scratch.file("copy of a.epub", &bytes)).unwrap();

        assert_eq!(again, ImportResult::AlreadyImported(first));
        assert_eq!(library.books().unwrap().len(), 1);
    }

    #[test]
    fn epubs_that_cant_be_read_are_rejected_each_for_its_reason() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let drm = r#"<encryption xmlns:enc="http://www.w3.org/2001/04/xmlenc#"><enc:EncryptedData>
<enc:EncryptionMethod Algorithm="http://www.w3.org/2001/04/xmlenc#aes128-cbc"/></enc:EncryptedData></encryption>"#;
        let fixed = Fixture::new(r#"<meta property="rendition:layout">pre-paginated</meta>"#);
        let drm = emma().file("META-INF/encryption.xml", drm.as_bytes());
        let no_mimetype = crate::epub::fixtures::zip(&[("a.txt", b"a")], &[]);
        let cases: [(&str, Vec<u8>, epub::Error); 5] = [
            ("drm.epub", drm.bytes(), epub::Error::DrmProtected),
            ("fixed.epub", fixed.bytes(), epub::Error::FixedLayout),
            ("garbage.epub", b"not a zip".to_vec(), epub::Error::Invalid),
            ("empty.epub", Vec::new(), epub::Error::Invalid),
            ("zip.zip", no_mimetype, epub::Error::Invalid),
        ];

        for (name, bytes, reason) in cases {
            let rejected = import(&library, &scratch.file(name, &bytes));
            assert!(
                matches!(&rejected, Err(ImportError::Epub(why)) if *why == reason),
                "{name}: {rejected:?}"
            );
        }

        assert_eq!(library.books().unwrap(), []);
        let folders = fs::read_dir(scratch.0.join("library")).unwrap();
        let stored = folders.filter(|e| {
            !e.as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with('.')
        });
        assert_eq!(stored.count(), 0);
        let staged = fs::read_dir(scratch.0.join("library/.staging")).unwrap();
        assert_eq!(staged.count(), 0);
    }

    #[test]
    fn an_epub_opens_to_its_spine_in_reading_order_with_each_items_content() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let book = emma()
            .manifest(r#"<item id="css" href="style.css" media-type="text/css"/>"#)
            .file("OEBPS/style.css", b"p{}");
        let book = import_new(&library, &scratch.file("emma.epub", &book.bytes()));

        let epub = library.open_epub(&book.id).unwrap().unwrap();

        assert_eq!(
            epub.spine(),
            ["OEBPS/text/chapter0.xhtml", "OEBPS/text/chapter1.xhtml"]
        );
        let second = epub.resource("OEBPS/text/chapter1.xhtml").unwrap();
        assert_eq!(second.media_type, "application/xhtml+xml");
        assert!(String::from_utf8(second.bytes).unwrap().contains("world"));
        let css = epub.resource("OEBPS/style.css").unwrap();
        assert_eq!(css.media_type, "text/css");
        assert_eq!(css.bytes, b"p{}");
    }

    #[test]
    fn an_epub_opens_to_its_table_of_contents_pointing_into_its_text() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let nav = r#"<html xmlns:epub="x"><body><nav epub:type="toc"><ol><li><a href="text/chapter1.xhtml">Second</a></li></ol></nav></body></html>"#;
        let book = emma()
            .manifest(r#"<item id="n" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>"#)
            .file("OEBPS/nav.xhtml", nav.as_bytes());
        let book = import_new(&library, &scratch.file("emma.epub", &book.bytes()));

        let contents = library.open_epub(&book.id).unwrap().unwrap().contents();

        assert_eq!(contents.len(), 1);
        assert_eq!(contents[0].title, "Second");
        assert_eq!(
            contents[0].place,
            Some(Place::Epub {
                spine_index: 1,
                offset: 0
            })
        );
    }

    #[test]
    fn only_an_epub_in_the_library_opens() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let pdf = import_new(&library, &scratch.file("a.pdf", PDF));

        assert!(library.open_epub(&pdf.id).unwrap().is_none());
        assert!(library.open_epub(NO_BOOK).unwrap().is_none());
        assert!(library.open_epub("../x").is_err());
    }

    #[test]
    fn an_epub_sorts_and_deletes_like_a_pdf() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let pdf = import_new(&library, &scratch.file("a.pdf", PDF));
        let epub = import_new(&library, &scratch.file("b.epub", &emma().bytes()));
        assert_eq!(library.books().unwrap(), [epub.clone(), pdf]);

        library.delete(&epub.id).unwrap();

        assert_eq!(library.books().unwrap().len(), 1);
        assert!(!epub.path.exists());
    }

    #[test]
    fn a_failed_import_leaves_the_library_unchanged() {
        let scratch = Scratch::new();
        let library = scratch.library();
        import_new(&library, &scratch.file("a.pdf", PDF));

        assert!(
            library
                .import(&scratch.0.join("missing.pdf"), &one_page, |_, _| {})
                .is_err()
        );

        assert_eq!(library.books().unwrap().len(), 1);
        assert_eq!(visible_entries(&scratch.0.join("library")), 1);
        assert_eq!(
            fs::read_dir(scratch.0.join("library/.staging"))
                .unwrap()
                .count(),
            0
        );
    }

    #[test]
    fn open_removes_leftover_staging_folders() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let leftover = scratch.0.join("library/.staging/crashed");
        fs::create_dir_all(&leftover).unwrap();
        fs::write(leftover.join("half.pdf"), b"ab").unwrap();
        drop(library);

        let library = scratch.library();

        assert!(!leftover.exists());
        assert_eq!(library.books().unwrap(), vec![]);
    }

    #[test]
    fn importing_the_same_bytes_again_reports_the_existing_book() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let first = import_new(&library, &scratch.file("a.pdf", PDF));

        let again = import(&library, &scratch.file("b.pdf", PDF)).unwrap();

        assert_eq!(again, ImportResult::AlreadyImported(first));
    }

    #[test]
    fn a_duplicate_import_leaves_the_library_unchanged() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let first = import_new(&library, &scratch.file("a.pdf", PDF));

        import(&library, &scratch.file("renamed.pdf", PDF)).unwrap();

        assert_eq!(library.books().unwrap(), vec![first.clone()]);
        // The File is the folder's only visible entry: no copy of the duplicate.
        let folder = first.path.parent().unwrap();
        assert_eq!(visible_entries(folder), 1);
        assert_eq!(
            fs::read_dir(scratch.0.join("library/.staging"))
                .unwrap()
                .count(),
            0
        );
    }

    #[test]
    fn delete_deletes_the_book_and_its_file() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let keep = import_new(&library, &scratch.file("a.pdf", b"%PDF-first"));
        let gone = import_new(&library, &scratch.file("b.pdf", b"%PDF-second"));

        library.delete(&gone.id).unwrap();

        assert_eq!(library.books().unwrap(), vec![keep]);
        assert!(!gone.path.exists());
        assert_eq!(
            fs::read_dir(scratch.0.join("library/.staging"))
                .unwrap()
                .count(),
            0
        );
    }

    #[test]
    fn deleting_a_book_that_is_not_there_does_nothing() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let kept = import_new(&library, &scratch.file("a.pdf", PDF));

        library.delete(NO_BOOK).unwrap();

        assert_eq!(library.books().unwrap(), vec![kept]);
    }

    #[test]
    fn delete_refuses_an_id_that_points_outside_the_library() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let outside = scratch.0.join("outside");
        fs::create_dir(&outside).unwrap();

        for id in ["../outside", "", ".", ".staging", "a/b"] {
            let err = library.delete(id).unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::InvalidInput, "id {id:?}");
        }
        assert!(outside.exists());
        assert!(scratch.0.join("library/.staging").exists());
    }

    #[test]
    fn a_deleted_book_can_be_imported_again() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let source = scratch.file("a.pdf", PDF);
        let first = import_new(&library, &source);
        library.delete(&first.id).unwrap();

        let again = import_new(&library, &source);

        assert_eq!(again.path, first.path);
        assert_eq!(library.books().unwrap(), vec![again]);
    }

    /// The path of the state of `book`.
    fn state_path(library: &Library, book: &Book) -> PathBuf {
        library.folder.state_file(&book.id)
    }

    fn progress(page_index: u32, last_read: u64) -> ReadingProgress {
        ReadingProgress::pdf(page_index, 12.5, 700.25, last_read)
    }

    /// The Reading Progress `library` lists for `id`.
    fn listed_progress(library: &Library, id: &str) -> Option<ReadingProgress> {
        let books = library.books().unwrap();
        books.into_iter().find(|d| d.id == id).unwrap().progress
    }

    #[test]
    fn a_never_opened_book_lists_without_reading_progress() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let book = import_new(&library, &scratch.file("a.pdf", PDF));

        assert_eq!(book.progress, None);
        assert_eq!(listed_progress(&library, &book.id), None);
    }

    #[test]
    fn saved_reading_progress_is_listed_and_survives_reopening_the_library() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let book = import_new(&library, &scratch.file("a.pdf", PDF));

        library
            .save_progress(&book.id, &progress(7, 1_000), by_date())
            .unwrap();
        assert_eq!(
            listed_progress(&library, &book.id),
            Some(progress(7, 1_000))
        );
        drop(library);

        let library = scratch.library();
        assert_eq!(
            listed_progress(&library, &book.id),
            Some(progress(7, 1_000))
        );
    }

    #[test]
    fn saving_again_replaces_the_reading_progress_as_a_whole() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let book = import_new(&library, &scratch.file("a.pdf", PDF));

        library
            .save_progress(&book.id, &progress(7, 1_000), by_date())
            .unwrap();
        library
            .save_progress(&book.id, &progress(2, 2_000), by_date())
            .unwrap();

        assert_eq!(
            listed_progress(&library, &book.id),
            Some(progress(2, 2_000))
        );
    }

    #[test]
    fn the_state_and_progress_files_do_not_hide_the_file() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let book = import_new(&library, &scratch.file("a.pdf", PDF));
        library
            .save_progress(&book.id, &progress(1, 1), by_date())
            .unwrap();
        highlight(&library, &book.id, "the passage", 1);

        let listed = library.books().unwrap();

        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].path, book.path);
    }

    #[test]
    fn the_state_is_a_hidden_file_holding_the_date_imported_and_the_markup() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let book = import_new(&library, &scratch.file("a.pdf", PDF));
        let added = highlight(&library, &book.id, "the passage", 1);

        let state = BookState::decode(&fs::read(state_path(&library, &book)).unwrap()).unwrap();

        assert_eq!(state.imported, book.imported);
        assert_eq!(state.markup.highlights, vec![added]);
        assert!(state_path(&library, &book).starts_with(scratch.0.join("library/.state")));
    }

    #[test]
    fn saving_leaves_nothing_in_staging() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let book = import_new(&library, &scratch.file("a.pdf", PDF));

        library
            .save_progress(&book.id, &progress(1, 1), by_date())
            .unwrap();

        let staging = scratch.0.join("library/.staging");
        assert_eq!(fs::read_dir(staging).unwrap().count(), 0);
    }

    #[test]
    fn saving_for_an_unknown_book_does_nothing_and_creates_no_state() {
        let scratch = Scratch::new();
        let library = scratch.library();

        library
            .save_progress(NO_BOOK, &progress(1, 1), by_date())
            .unwrap();

        assert!(!library.folder.state_file(NO_BOOK).exists());
        assert_eq!(library.books().unwrap(), vec![]);
    }

    /// The names of the Files in the Library, sorted.
    fn file_names(scratch: &Scratch) -> Vec<String> {
        let mut names: Vec<_> = fs::read_dir(scratch.0.join("library"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| !name.starts_with('.'))
            .collect();
        names.sort();
        names
    }

    #[test]
    fn the_library_folder_shows_only_the_books_and_hides_state_and_details() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let book = import_new(&library, &scratch.file("a.pdf", PDF));
        library
            .save_progress(&book.id, &progress(1, 1), by_date())
            .unwrap();
        highlight(&library, &book.id, "the passage", 1);
        library.set_default_highlight_color("Blue").unwrap();

        assert_eq!(file_names(&scratch), ["a.pdf"]);
    }

    #[test]
    fn a_taken_name_gets_2_then_3() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let names: Vec<_> = ["one", "two", "three"]
            .map(|text| {
                let bytes = [PDF, text.as_bytes()].concat();
                let source = scratch.file("same.pdf", &bytes);
                import_new(&library, &source).path
            })
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();

        assert_eq!(names, ["same.pdf", "same 2.pdf", "same 3.pdf"]);
        assert_eq!(
            fs::read(scratch.0.join("library/same 2.pdf")).unwrap(),
            [PDF, b"two"].concat()
        );
    }

    #[test]
    fn a_book_renamed_outside_the_app_keeps_its_id_markup_and_progress() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let book = import_new(&library, &scratch.file("old.pdf", PDF));
        let added = highlight(&library, &book.id, "the passage", 1);
        library
            .save_progress(&book.id, &progress(3, 9), by_date())
            .unwrap();

        fs::rename(&book.path, scratch.0.join("library/new.pdf")).unwrap();

        let listed = library.books().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, book.id);
        assert_eq!(listed[0].path, scratch.0.join("library/new.pdf"));
        assert_eq!(listed[0].progress, Some(progress(3, 9)));
        assert_eq!(library.markup(&book.id).unwrap().highlights, vec![added]);
        // Also after reopening, and the File can be Imported as the same Book.
        let library = scratch.library();
        assert_eq!(library.books().unwrap(), listed);
        let again = import(&library, &scratch.file("elsewhere.pdf", PDF)).unwrap();
        assert!(matches!(again, ImportResult::AlreadyImported(b) if b.id == book.id));
    }

    #[test]
    fn a_book_renamed_and_edited_outside_the_app_is_a_new_book() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let book = import_new(&library, &scratch.file("old.pdf", PDF));
        highlight(&library, &book.id, "the passage", 1);

        fs::remove_file(&book.path).unwrap();
        fs::write(
            scratch.0.join("library/new.pdf"),
            [PDF, b" edited"].concat(),
        )
        .unwrap();

        let listed = library.books().unwrap();
        assert_eq!(listed.len(), 1);
        assert_ne!(listed[0].id, book.id);
        assert_eq!(library.markup(&listed[0].id).unwrap(), Markup::default());
        assert_eq!(library.markup(&book.id).unwrap(), Markup::default());
    }

    #[test]
    fn a_book_edited_outside_the_app_under_its_name_stays_the_book() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let book = import_new(&library, &scratch.file("a.pdf", PDF));
        fs::write(&book.path, [PDF, b" edited"].concat()).unwrap();

        let listed = library.books().unwrap();

        assert_eq!(listed.iter().map(|b| &b.id).collect::<Vec<_>>(), [&book.id]);
    }

    #[test]
    fn a_file_put_in_the_folder_outside_the_app_is_a_new_book_and_a_file_taken_out_is_deleted() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let book = import_new(&library, &scratch.file("a.pdf", PDF));
        fs::write(scratch.0.join("library/b.pdf"), b"%PDF-b").unwrap();
        fs::write(scratch.0.join("library/notes.txt"), b"no book").unwrap();

        let listed = library.books().unwrap();
        assert_eq!(listed.len(), 2);
        assert!(listed.iter().any(|b| b.id == book.id));

        fs::remove_file(&book.path).unwrap();
        let listed = library.books().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].path, scratch.0.join("library/b.pdf"));
        assert!(library.book(&book.id).unwrap().is_none());
        assert!(!library.folder.state_file(&book.id).exists());
    }

    #[test]
    fn importing_a_file_that_was_put_in_the_folder_outside_the_app_gives_that_book() {
        let scratch = Scratch::new();
        let library = scratch.library();
        fs::write(scratch.0.join("library/put.pdf"), PDF).unwrap();

        let result = import(&library, &scratch.file("a.pdf", PDF)).unwrap();

        let ImportResult::AlreadyImported(book) = result else {
            panic!("{result:?}");
        };
        assert_eq!(book.path, scratch.0.join("library/put.pdf"));
        assert_eq!(library.books().unwrap(), vec![book]);
    }

    #[test]
    fn two_states_of_one_file_merge_into_the_lower_item_id() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let book = import_new(&library, &scratch.file("a.pdf", PDF));
        let added = highlight(&library, &book.id, "the passage", 1);
        // The same File, imported on another Device before they synced.
        let sha256 = BookState::decode(&fs::read(state_path(&library, &book)).unwrap())
            .unwrap()
            .sha256;
        fs::write(scratch.0.join("library/a 2.pdf"), PDF).unwrap();
        let twin = |id: &str| BookState {
            id: id.into(),
            file_name: "a 2.pdf".into(),
            sha256: sha256.clone(),
            imported: book.imported + 1,
            markup: Markup::default(),
            progress: None,
            changes: Default::default(),
        };
        let (low, high) = ("aaaaaaaaaaaa", "zzzzzzzzzzzy");
        // The twin has the lower ID, the Book a higher one: they swap.
        let mut kept = BookState::decode(&fs::read(state_path(&library, &book)).unwrap()).unwrap();
        fs::remove_file(state_path(&library, &book)).unwrap();
        kept.id = high.into();
        library.folder.write_state(&kept).unwrap();
        library.folder.write_state(&twin(low)).unwrap();

        let listed = library.books().unwrap();

        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, low);
        assert_eq!(library.markup(low).unwrap().highlights, vec![added]);
        // The lower Item ID's File stays, the spare copy goes.
        assert_eq!(file_names(&scratch), ["a 2.pdf"]);
        assert!(!library.folder.state_file(high).exists());
    }

    fn details(title: Option<&str>, author: Option<&str>, page_count: u32) -> Details {
        Details {
            title: title.map(str::to_owned),
            author: author.map(str::to_owned),
            size: Size::Pages(page_count),
        }
    }

    #[test]
    fn import_keeps_the_details_the_reader_gives_and_they_survive_reopening() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let moby = details(Some("Moby-Dick"), Some("Melville"), 635);

        let book = import_with(
            &library,
            &scratch.file("a.pdf", PDF),
            &reading(moby.clone()),
        );

        assert_eq!(book.details, Some(moby.clone()));
        assert_eq!(library.books().unwrap()[0].details, Some(moby.clone()));
        let reopened = scratch.library();
        assert_eq!(reopened.books().unwrap()[0].details, Some(moby));
    }

    #[test]
    fn a_pdf_is_read_before_anything_is_copied() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let source = scratch.file("a.pdf", PDF);
        let stored = scratch.0.join("library");
        let read = AtomicBool::new(false);
        let reader = |path: &Path| -> Result<Details, PdfProblem> {
            assert_eq!(path, source.as_path());
            assert_eq!(visible_entries(&stored), 0);
            read.store(true, Relaxed);
            Ok(details(None, None, 1))
        };

        import_with(&library, &source, &reader);
        assert!(read.load(Relaxed));
    }

    #[test]
    fn a_pdf_the_reader_refuses_is_refused_for_its_reason_and_nothing_is_copied() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let source = scratch.file("damaged.pdf", b"%PDF-1.7 damaged");

        let unreadable = library.import(&source, &refusing(PdfProblem::Unreadable), |_, _| {});
        assert!(
            matches!(unreadable, Err(ImportError::UnreadablePdf)),
            "{unreadable:?}"
        );
        let locked = library.import(&source, &refusing(PdfProblem::Locked), |_, _| {});
        assert!(matches!(locked, Err(ImportError::LockedPdf)), "{locked:?}");

        assert_eq!(library.books().unwrap(), []);
        assert_eq!(visible_entries(&scratch.0.join("library")), 0);
        let staged = fs::read_dir(scratch.0.join("library/.staging")).unwrap();
        assert_eq!(staged.count(), 0);
    }

    #[test]
    fn an_epub_is_read_by_the_core_and_never_by_the_reader() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let source = scratch.file("book.epub", &emma().bytes());
        let never = |_: &Path| -> Result<Details, PdfProblem> {
            panic!("an EPUB is not read by the reader")
        };

        let book = import_with(&library, &source, &never);

        assert_eq!(book.format(), Format::Epub);
        assert_eq!(book.title(), "Emma");
    }

    #[test]
    fn books_without_details_are_listed_newest_first_and_only_those() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let old = import_new(&library, &scratch.file("old.pdf", b"%PDF-old"));
        a_millisecond_later();
        let new = import_new(&library, &scratch.file("new.pdf", b"%PDF-new"));
        a_millisecond_later();
        let epub = import_new(&library, &scratch.file("book.epub", &emma().bytes()));
        let kept = import_new(&library, &scratch.file("kept.pdf", b"%PDF-kept"));
        let library = forget_details(&scratch, library, &[&old.id, &new.id, &epub.id]);

        assert_eq!(
            library.books_lacking_details(),
            vec![epub.id.clone(), new.id.clone(), old.id.clone()]
        );
        assert!(!library.books_lacking_details().contains(&kept.id));
    }

    #[test]
    fn read_details_reads_a_pdf_without_details_and_keeps_them() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let book = import_new(&library, &scratch.file("a.pdf", PDF));
        let library = forget_details(&scratch, library, &[&book.id]);
        let moby = details(Some("Moby-Dick"), Some("Melville"), 635);

        assert!(
            library
                .read_details(&book.id, &reading(moby.clone()))
                .unwrap()
        );

        let read = library.book(&book.id).unwrap().unwrap();
        assert_eq!(read.details, Some(moby.clone()));
        let reopened = scratch.library().book(&book.id).unwrap().unwrap();
        assert_eq!(reopened.details, Some(moby));
        assert!(library.books_lacking_details().is_empty());
    }

    #[test]
    fn a_pdf_the_reader_refuses_keeps_no_details_and_is_listed_again() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let book = import_new(&library, &scratch.file("a.pdf", PDF));
        let library = forget_details(&scratch, library, &[&book.id]);

        let kept = library
            .read_details(&book.id, &refusing(PdfProblem::Locked))
            .unwrap();

        assert!(!kept);
        assert_eq!(library.book(&book.id).unwrap().unwrap().details, None);
        assert_eq!(library.books_lacking_details(), vec![book.id.clone()]);
    }

    #[test]
    fn read_details_leaves_the_details_a_book_has() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let moby = details(Some("Moby-Dick"), None, 1);
        let book = import_with(
            &library,
            &scratch.file("a.pdf", PDF),
            &reading(moby.clone()),
        );
        let other = reading(details(Some("Other"), None, 9));

        assert!(!library.read_details(&book.id, &other).unwrap());

        let kept = library.book(&book.id).unwrap().unwrap();
        assert_eq!(kept.details, Some(moby));
    }

    #[test]
    fn read_details_reads_nothing_outside_the_library() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let read = reading(details(None, None, 1));

        assert!(!library.read_details(NO_BOOK, &read).unwrap());
        assert!(library.read_details("../x", &read).is_err());
    }

    #[test]
    fn a_book_edited_outside_the_app_keeps_its_id_and_has_its_details_read_again() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let moby = details(Some("Moby-Dick"), None, 1);
        let book = import_with(&library, &scratch.file("a.pdf", PDF), &reading(moby));
        assert!(library.books_lacking_details().is_empty());

        fs::write(&book.path, b"%PDF-edited").unwrap();
        let later = SystemTime::now() + Duration::from_secs(1);
        let file = File::options().write(true).open(&book.path).unwrap();
        file.set_modified(later).unwrap();

        let listed = library.books().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, book.id);
        assert_eq!(listed[0].details, None);
        assert_eq!(library.books_lacking_details(), vec![book.id.clone()]);
    }

    #[test]
    fn an_epub_put_in_the_folder_is_read_by_the_core_and_listed_as_an_epub() {
        let scratch = Scratch::new();
        let library = scratch.library();
        fs::write(scratch.0.join("library/emma.epub"), emma().bytes()).unwrap();
        let book = library.books().unwrap().remove(0);
        let never = |_: &Path| -> Result<Details, PdfProblem> { panic!("an EPUB is not a PDF") };

        assert_eq!(library.books_lacking_details(), vec![book.id.clone()]);
        assert!(library.read_details(&book.id, &never).unwrap());

        let read = library.book(&book.id).unwrap().unwrap();
        assert_eq!(read.format(), Format::Epub);
        assert_eq!(read.title(), "Emma");
        assert!(library.books_lacking_details().is_empty());
    }

    #[test]
    fn the_pass_reads_the_details_of_a_pdf_that_lacks_them_and_tells_the_gallery() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let book = import_new(&library, &scratch.file("a.pdf", PDF));
        let library = Arc::new(forget_details(&scratch, library, &[&book.id]));
        let moby = details(Some("Moby-Dick"), Some("Melville"), 635);
        let told = Gallery::default();

        let pass = DetailsPass::start(
            library.clone(),
            Arc::new(reading(moby.clone())),
            Arc::new(told.clone()),
        );

        told.0.wait_until(|| told.0.count() >= 1);
        assert_eq!(library.book(&book.id).unwrap().unwrap().details, Some(moby));
        drop(pass);
    }

    #[test]
    fn the_pass_tells_the_gallery_once_per_batch_not_once_per_pdf_and_reads_none_twice() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let ids: Vec<String> = (0..17)
            .map(|n| {
                let bytes = [b"%PDF-", n.to_string().as_bytes()].concat();
                import_new(&library, &scratch.file(&format!("{n}.pdf"), &bytes)).id
            })
            .collect();
        let ids: Vec<&String> = ids.iter().collect();
        let library = Arc::new(forget_details(&scratch, library, &ids));
        let reads = Reads::default();
        let told = Gallery::default();

        let pass = DetailsPass::start(
            library.clone(),
            Arc::new(reads.clone()),
            Arc::new(told.clone()),
        );

        reads.0.wait_until(|| reads.0.count() >= 17);
        // A pass runs once.
        drop(pass);
        assert_eq!(reads.0.count(), 17);
        // 16 reads are one telling, the leftover read another.
        assert!(told.0.count() >= 2);
    }

    #[test]
    fn the_pass_tells_the_gallery_nothing_for_a_pdf_it_refuses_and_retries_it_later() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let book = import_new(&library, &scratch.file("a.pdf", PDF));
        let library = Arc::new(forget_details(&scratch, library, &[&book.id]));
        let moby = details(Some("Moby-Dick"), None, 1);
        let reader = RefusesOnce::new(moby.clone());
        let told = Gallery::default();

        let pass = DetailsPass::start(
            library.clone(),
            Arc::new(reader.clone()),
            Arc::new(told.clone()),
        );
        reader.0.wait_until(|| reader.0.count() >= 1);

        // Nothing was kept, so the Gallery is not told.
        assert_eq!(library.book(&book.id).unwrap().unwrap().details, None);
        assert_eq!(told.0.count(), 0);

        // A later pass tries the PDF again.
        drop(pass);
        let pass = DetailsPass::start(
            library.clone(),
            Arc::new(reader.clone()),
            Arc::new(told.clone()),
        );
        told.0.wait_until(|| told.0.count() >= 1);
        assert_eq!(library.book(&book.id).unwrap().unwrap().details, Some(moby));
        drop(pass);
    }

    #[test]
    fn the_listing_orders_and_searches_the_books_by_id() {
        use crate::listing::SortKey;
        let scratch = Scratch::new();
        let library = scratch.library();
        let mut ids = vec![];
        for (n, title) in ["Zebra", "apple 10", "apple 2"].into_iter().enumerate() {
            let source = scratch.file("s.pdf", format!("%PDF-{n}").as_bytes());
            let book = import_new(&library, &source);
            let titled = details(Some(title), None, 1);
            library.set_details(&book.id, &titled).unwrap();
            ids.push(book.id);
            sleep(Duration::from_millis(2));
        }
        let sort = |key, reversed| Sort { key, reversed };
        let by = |key, reversed, search: &str| -> Vec<String> {
            let (books, listing) = library.listing(sort(key, reversed)).unwrap();
            let matched = listing.matching(search);
            matched.into_iter().map(|i| books[i].id.clone()).collect()
        };
        let [zebra, ten, two] = [&ids[0], &ids[1], &ids[2]].map(String::as_str);

        assert_eq!(by(SortKey::DateImported, false, ""), [two, ten, zebra]);
        assert_eq!(by(SortKey::Title, false, ""), [two, ten, zebra]);
        assert_eq!(by(SortKey::Title, true, ""), [zebra, ten, two]);
        assert_eq!(by(SortKey::Title, false, "APPLE 1"), [ten]);
    }

    #[test]
    fn an_opened_epubs_lengths_are_its_stored_character_counts_and_no_item_is_scanned() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let bytes = emma()
            .chapters(&["<p>Hello</p>", "<p>Gr\u{fc}\u{df}e \u{1f600}</p>"])
            .bytes();
        let book = import_new(&library, &scratch.file("book.epub", &bytes));

        let epub = library.open_epub(&book.id).unwrap().unwrap();

        let Some(Size::Characters(stored)) = book.details.map(|d| d.size) else {
            panic!("an EPUB is sized in characters");
        };
        assert_eq!(epub.characters(), stored);
        assert_eq!(epub.scanned_items(), 0);
        assert_eq!(stored, [5, 8]);
        // What scanning the File gives, which the stored counts must match.
        assert_eq!(Epub::open(&book.path).unwrap().characters(), stored);
    }

    #[test]
    fn details_are_kept_as_stored() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let book = import_new(&library, &scratch.file("a.pdf", PDF));

        library
            .set_details(&book.id, &details(Some(" "), Some(" Melville\n"), 3))
            .unwrap();

        let stored = library.book(&book.id).unwrap().unwrap().details;
        assert_eq!(stored, Some(details(None, Some("Melville"), 3)));
    }

    #[test]
    fn details_of_a_deleted_book_are_gone_and_not_written() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let source = scratch.file("a.pdf", PDF);
        let book = import_new(&library, &source);
        let moby = details(Some("Moby-Dick"), None, 1);
        library.set_details(&book.id, &moby).unwrap();
        library.delete(&book.id).unwrap();

        library.set_details(&book.id, &moby).unwrap();

        assert!(!library.folder.state_file(&book.id).exists());
        assert!(!library.folder.details(&book.id).exists());
        assert_eq!(
            import_new(&library, &source).details,
            Some(details(None, None, 1))
        );
    }

    #[test]
    fn a_reimport_has_the_details_the_file_has_now_not_stale_ones() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let source = scratch.file("a.pdf", PDF);
        let first = details(Some("Moby-Dick"), None, 1);
        let second = details(Some("Dracula"), Some("Stoker"), 2);
        let book = import_with(&library, &source, &reading(first.clone()));
        assert_eq!(library.books().unwrap()[0].details, Some(first));

        library.delete(&book.id).unwrap();
        assert!(library.books().unwrap().is_empty());
        let again = import_with(&library, &source, &reading(second.clone()));

        assert_eq!(again.details, Some(second.clone()));
        assert_eq!(library.books().unwrap()[0].details, Some(second));
    }

    #[test]
    fn small_libraries_list_on_one_thread_and_large_ones_in_big_chunks() {
        assert_eq!(chunk_size(0, 8), MIN_PER_THREAD);
        assert_eq!(chunk_size(31, 8), MIN_PER_THREAD);
        assert_eq!(chunk_size(100, 8), MIN_PER_THREAD);
        assert_eq!(chunk_size(1000, 8), 125);
        assert_eq!(chunk_size(1000, 1), 1000);
    }

    #[test]
    fn setting_details_refuses_an_id_that_points_outside_the_library() {
        let scratch = Scratch::new();
        let library = scratch.library();
        for id in ["../x", "/tmp", "", ".staging"] {
            let err = library
                .set_details(id, &details(None, None, 1))
                .unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::InvalidInput, "{id:?}");
        }
    }

    #[test]
    fn the_title_is_the_stored_one_or_else_the_file_name_without_extension() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let book = import_new(&library, &scratch.file("scan0042.pdf", PDF));
        assert_eq!(book.title(), "scan0042");

        library
            .set_details(&book.id, &details(None, Some("Melville"), 1))
            .unwrap();
        assert_eq!(library.books().unwrap()[0].title(), "scan0042");

        library
            .set_details(&book.id, &details(Some("Moby-Dick"), None, 1))
            .unwrap();
        assert_eq!(library.books().unwrap()[0].title(), "Moby-Dick");
    }

    #[test]
    fn percent_read_is_the_page_reached_of_the_page_count_rounded_down() {
        let book = |page_index, page_count| Book {
            id: String::new(),
            path: PathBuf::from("a.pdf"),
            sha256: String::new(),
            imported: 0,
            progress: Some(progress(page_index, 1)),
            details: Some(details(None, None, page_count)),
        };
        for (page_index, page_count, percent) in [
            (0, 4, 25),
            (3, 4, 100),
            (1, 3, 66),
            (0, 200, 0),
            (9, 4, 100),
        ] {
            let read = book(page_index, page_count).percent_read();
            assert_eq!(read, Some(percent), "page {page_index} of {page_count}");
        }
        assert_eq!(book(0, 0).percent_read(), None, "no pages");
        let never_opened = Book {
            progress: None,
            ..book(0, 4)
        };
        assert_eq!(never_opened.percent_read(), None);
        let unknown_page_count = Book {
            details: None,
            ..book(0, 4)
        };
        assert_eq!(unknown_page_count.percent_read(), None);
    }

    #[test]
    fn epub_reading_progress_is_saved_listed_and_survives_reopening_the_library() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let book = import_new(&library, &scratch.file("book.epub", &emma().bytes()));
        assert_eq!(book.progress, None);

        library
            .save_progress(&book.id, &ReadingProgress::epub(1, 3, 1_000), by_date())
            .unwrap();
        library
            .save_progress(&book.id, &ReadingProgress::epub(1, 4, 2_000), by_date())
            .unwrap();

        for library in [&library, &scratch.library()] {
            let listed = listed_progress(library, &book.id);
            assert_eq!(listed, Some(ReadingProgress::epub(1, 4, 2_000)));
        }
    }

    #[test]
    fn percent_read_of_an_epub_is_the_characters_reached_of_all_rounded_down() {
        let book = |spine_index, offset| Book {
            id: String::new(),
            path: PathBuf::from("a.epub"),
            sha256: String::new(),
            imported: 0,
            progress: Some(ReadingProgress::epub(spine_index, offset, 1)),
            details: Some(Details {
                title: None,
                author: None,
                size: Size::Characters(vec![30, 60, 10]),
            }),
        };
        // The characters before the item count too, and only what is read.
        for (spine_index, offset, percent) in [
            (0, 0, 0),
            (0, 29, 29),
            (1, 0, 30),
            (1, 30, 60),
            (2, 5, 95),
            (2, 10, 100),
            (2, 99, 100),
            (9, 0, 100),
        ] {
            let read = book(spine_index, offset).percent_read();
            assert_eq!(read, Some(percent), "item {spine_index}, offset {offset}");
        }
        let pdf_progress = Book {
            progress: Some(progress(0, 1)),
            ..book(0, 0)
        };
        assert_eq!(pdf_progress.percent_read(), None, "another Format's place");
        let empty = Book {
            details: Some(Details {
                size: Size::Characters(vec![0]),
                ..book(0, 0).details.unwrap()
            }),
            ..book(0, 0)
        };
        assert_eq!(empty.percent_read(), None, "no text");
    }

    #[test]
    fn a_book_is_found_by_id_while_it_is_in_the_library() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let book = import_new(&library, &scratch.file("a.pdf", PDF));
        library
            .save_progress(&book.id, &progress(1, 1), by_date())
            .unwrap();

        let found = library.book(&book.id).unwrap();
        assert_eq!(found, Some(library.books().unwrap().remove(0)));

        library.delete(&book.id).unwrap();
        assert_eq!(library.book(&book.id).unwrap(), None);
        assert_eq!(
            library.book("../x").unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
    }

    #[test]
    fn saving_for_a_deleted_book_does_nothing() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let book = import_new(&library, &scratch.file("a.pdf", PDF));
        library.delete(&book.id).unwrap();

        library
            .save_progress(&book.id, &progress(1, 1), by_date())
            .unwrap();

        assert!(!library.folder.state_file(&book.id).exists());
        assert!(!library.folder.details(&book.id).exists());
    }

    #[test]
    fn saving_refuses_an_id_that_points_outside_the_library() {
        let scratch = Scratch::new();
        let library = scratch.library();

        for id in ["../outside", "", ".staging", "a/b"] {
            let err = library
                .save_progress(id, &progress(1, 1), by_date())
                .unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::InvalidInput, "id {id:?}");
        }
    }

    #[test]
    fn delete_deletes_the_state_along_with_the_file() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let book = import_new(&library, &scratch.file("a.pdf", PDF));
        library
            .save_progress(&book.id, &progress(3, 5), by_date())
            .unwrap();

        library.delete(&book.id).unwrap();
        assert_eq!(library.books().unwrap(), vec![]);

        // Imported again, the Book is as good as new.
        let again = import_new(&library, &scratch.file("a.pdf", PDF));
        assert_eq!(again.progress, None);
    }

    #[test]
    fn unreadable_reading_progress_lists_as_never_opened_and_is_replaced_by_the_next_save() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let book = import_new(&library, &scratch.file("a.pdf", PDF));
        library
            .save_progress(&book.id, &progress(3, 5), by_date())
            .unwrap();
        let state = state_path(&library, &book);
        let stored = String::from_utf8(fs::read(&state).unwrap()).unwrap();
        fs::write(&state, stored.replace("3 12.5 700.25 5", "garbage")).unwrap();

        assert_eq!(listed_progress(&scratch.library(), &book.id), None);

        library
            .save_progress(&book.id, &progress(4, 6), by_date())
            .unwrap();
        assert_eq!(listed_progress(&library, &book.id), Some(progress(4, 6)));
    }

    /// Saves the Reading Progress of page `page` read at `last_read` for
    /// Book `id`, and says whether that changed what the Gallery for `sort`
    /// shows.
    fn save_at(library: &Library, id: &str, page: u32, last_read: u64, sort: Sort) -> bool {
        library
            .save_progress(id, &progress(page, last_read), sort)
            .unwrap()
    }

    /// What `sort` lists of the Library, in order, with each percent read.
    fn gallery(library: &Library, sort: Sort) -> Vec<(String, Option<u8>)> {
        let (books, listing) = library.listing(sort).unwrap();
        listing
            .matching("")
            .into_iter()
            .map(|i| {
                let book = &books[i];
                (book.id.clone(), book.percent_read())
            })
            .collect()
    }

    #[test]
    fn saving_progress_says_whether_the_gallery_shows_something_new_by_percent_read() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let ten_pages = reading(details(None, None, 10));
        let a = import_with(&library, &scratch.file("a.pdf", b"%PDF-a"), &ten_pages);
        let by_date = Sort {
            key: SortKey::DateImported,
            reversed: false,
        };

        assert!(save_at(&library, &a.id, 0, 100, by_date));
        // The same percent read: the Gallery shows what it did.
        assert!(!save_at(&library, &a.id, 0, 200, by_date));
        assert!(save_at(&library, &a.id, 4, 300, by_date));
    }

    #[test]
    fn saving_progress_says_whether_the_recently_read_order_changed() {
        let scratch = Scratch::new();
        let library = scratch.library();
        // No pages, so no percent read: only the order can change.
        let no_pages = reading(details(None, None, 0));
        let a = import_with(&library, &scratch.file("a.pdf", b"%PDF-a"), &no_pages);
        let b = import_with(&library, &scratch.file("b.pdf", b"%PDF-b"), &no_pages);
        let recent = Sort {
            key: SortKey::RecentlyRead,
            reversed: false,
        };

        assert!(save_at(&library, &a.id, 0, 100, recent));
        assert!(save_at(&library, &b.id, 0, 200, recent));
        // Already on top, and nothing else was read in between.
        assert!(!save_at(&library, &b.id, 0, 250, recent));
        // Past `b`.
        assert!(save_at(&library, &a.id, 0, 300, recent));
        assert_eq!(gallery(&library, recent)[0].0, a.id);
    }

    #[test]
    fn saving_progress_for_a_book_that_is_not_listed_says_nothing_changed() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let book = import_new(&library, &scratch.file("a.pdf", PDF));
        library.delete(&book.id).unwrap();

        assert!(
            !library
                .save_progress(&book.id, &progress(1, 1), by_date())
                .unwrap()
        );
    }

    #[test]
    fn a_save_that_says_the_gallery_is_unchanged_leaves_it_as_it_was_in_every_sort() {
        let sorts = [SortKey::DateImported, SortKey::Title, SortKey::RecentlyRead]
            .into_iter()
            .flat_map(|key| [false, true].map(|reversed| Sort { key, reversed }));
        // (Book, page, Last Read), the Books as named below.
        let saves = [
            (0, 0, 100),
            (0, 0, 150),
            (1, 3, 200),
            (1, 3, 250),
            (0, 7, 300),
            (2, 1, 350),
            (2, 1, 360),
            (1, 0, 400),
            (0, 7, 410),
            (2, 9, 420),
            (1, 9, 430),
            (0, 2, 440),
            (2, 4, 450),
        ];
        for sort in sorts {
            let scratch = Scratch::new();
            let library = scratch.library();
            let ten_pages = reading(details(None, None, 10));
            let ids: Vec<String> = ["a", "b", "c"]
                .into_iter()
                .map(|name| {
                    let source =
                        scratch.file(&format!("{name}.pdf"), format!("%PDF-{name}").as_bytes());
                    import_with(&library, &source, &ten_pages).id
                })
                .collect();

            let mut unchanged = 0;
            for (book, page, last_read) in saves {
                let before = gallery(&library, sort);
                let changed = save_at(&library, &ids[book], page, last_read, sort);
                if !changed {
                    unchanged += 1;
                    assert_eq!(gallery(&library, sort), before, "{sort:?}");
                }
            }
            assert!(unchanged > 0, "{sort:?}: nothing was called unchanged");
        }
    }

    fn anchor() -> Anchor {
        Anchor::Pdf(vec![
            PageRects {
                page_index: 2,
                rects: vec![Rect {
                    x: 72.0,
                    y: 700.5,
                    width: 200.0,
                    height: 12.0,
                }],
            },
            PageRects {
                page_index: 3,
                rects: vec![
                    Rect {
                        x: 72.0,
                        y: 720.0,
                        width: 100.0,
                        height: 12.0,
                    },
                    Rect {
                        x: 72.0,
                        y: 706.0,
                        width: 40.0,
                        height: 12.0,
                    },
                ],
            },
        ])
    }

    fn epub_anchor(start: (u32, u64), end: (u32, u64)) -> Anchor {
        let position = |(spine_index, offset)| EpubPosition {
            spine_index,
            offset,
        };
        Anchor::Epub {
            start: position(start),
            end: position(end),
        }
    }

    /// An imported EPUB whose chapters read "Call me Ishmael. 😀 Ishmael" and
    /// "Some years ago", in a Library of its own.
    fn moby(scratch: &Scratch) -> (Library, Book) {
        let library = scratch.library();
        let book = Fixture::new("").chapters(&[
            "<p>Call me Ishmael.</p><p> \u{1F600} Ishmael</p>",
            "<p>Some years ago</p>",
        ]);
        let book = import_new(&library, &scratch.file("moby.epub", &book.bytes()));
        (library, book)
    }

    #[test]
    fn an_epub_highlight_is_stored_with_its_positions_and_quoted_text_and_survives_reopening() {
        let scratch = Scratch::new();
        let (library, book) = moby(&scratch);

        let added = library
            .add_highlight(
                &book.id,
                epub_anchor((0, 8), (1, 4)),
                "Ishmael. \u{1F600} Ishmael\nSome",
                YELLOW,
                7,
            )
            .unwrap()
            .unwrap();
        library
            .set_highlight_comment(&book.id, &added.id, "whale")
            .unwrap();

        let reopened = scratch.library();
        let stored = reopened.markup(&book.id).unwrap().highlights;
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].anchor, epub_anchor((0, 8), (1, 4)));
        assert_eq!(stored[0].quoted_text, "Ishmael. \u{1F600} Ishmael\nSome");
        assert_eq!(stored[0].comment.as_deref(), Some("whale"));
        assert_eq!(stored[0].created, 7);
    }

    #[test]
    fn overlapping_epub_highlights_stay_separate() {
        let scratch = Scratch::new();
        let (library, book) = moby(&scratch);
        let add = |start, end, quoted: &str| {
            library
                .add_highlight(&book.id, epub_anchor(start, end), quoted, YELLOW, 1)
                .unwrap()
                .unwrap()
        };
        let (first, second) = (
            add((0, 0), (0, 7), "Call me"),
            add((0, 5), (0, 15), "e Ishmael"),
        );

        library.remove_highlight(&book.id, &first.id).unwrap();

        let left = library.markup(&book.id).unwrap().highlights;
        assert_eq!(left, vec![second]);
    }

    /// Where EPUB `book` of `library` says the passage `quoted` really is.
    fn relocated(library: &Library, book: &Book, anchor: Anchor, quoted: &str) -> Option<Anchor> {
        let Anchor::Epub { start, end } = anchor else {
            panic!("an EPUB anchor");
        };
        let (start, end) = library
            .open_epub(&book.id)
            .unwrap()
            .unwrap()
            .relocate(start, end, quoted)?;
        Some(Anchor::Epub { start, end })
    }

    #[test]
    fn an_epub_highlight_that_still_matches_its_text_stays_where_it_is() {
        let scratch = Scratch::new();
        let (library, book) = moby(&scratch);
        let anchor = epub_anchor((0, 8), (1, 4));
        let quoted = "Ishmael. \u{1F600} Ishmael\nSome";

        assert_eq!(
            relocated(&library, &book, anchor.clone(), quoted),
            Some(anchor)
        );
    }

    #[test]
    fn an_epub_highlight_whose_offsets_no_longer_match_is_found_by_its_quoted_text() {
        let scratch = Scratch::new();
        let (library, book) = moby(&scratch);
        let found = |anchor, quoted| relocated(&library, &book, anchor, quoted);

        // Within one spine item, in UTF-16 offsets (the emoji counts two).
        assert_eq!(
            found(epub_anchor((0, 0), (0, 3)), "Ishmael."),
            Some(epub_anchor((0, 8), (0, 16)))
        );
        // Across spine items.
        assert_eq!(
            found(epub_anchor((1, 99), (1, 99)), "Ishmael\nSome"),
            Some(epub_anchor((0, 20), (1, 4)))
        );
        // Of several occurrences, the one nearest to where it was.
        assert_eq!(
            found(epub_anchor((0, 17), (0, 24)), "Ishmael"),
            Some(epub_anchor((0, 20), (0, 27)))
        );
        assert_eq!(
            found(epub_anchor((0, 1), (0, 8)), "Ishmael"),
            Some(epub_anchor((0, 8), (0, 15)))
        );
    }

    #[test]
    fn an_epub_highlight_whose_text_is_nowhere_is_not_found() {
        let scratch = Scratch::new();
        let (library, book) = moby(&scratch);

        assert_eq!(
            relocated(&library, &book, epub_anchor((0, 8), (0, 15)), "Queequeg"),
            None
        );
        assert_eq!(
            relocated(&library, &book, epub_anchor((0, 8), (0, 15)), ""),
            None
        );
    }

    const YELLOW: Rgb = Rgb {
        r: 255,
        g: 235,
        b: 60,
    };

    fn highlight(library: &Library, id: &str, quoted_text: &str, created: u64) -> Highlight {
        library
            .add_highlight(id, anchor(), quoted_text, YELLOW, created)
            .unwrap()
            .unwrap()
    }

    #[test]
    fn a_book_without_markup_has_no_highlights() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let book = import_new(&library, &scratch.file("a.pdf", PDF));
        assert_eq!(library.markup(&book.id).unwrap(), Markup::default());
    }

    #[test]
    fn an_added_highlight_is_in_the_markup_and_survives_reopening_the_library() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let book = import_new(&library, &scratch.file("a.pdf", PDF));

        let added = highlight(&library, &book.id, "the passage", 1_000);

        assert_eq!(added.anchor, anchor());
        assert_eq!(added.quoted_text, "the passage");
        assert_eq!(added.color, YELLOW);
        assert_eq!(added.created, 1_000);
        let expected = Markup {
            highlights: vec![added],
            ..Markup::default()
        };
        assert_eq!(library.markup(&book.id).unwrap(), expected);
        drop(library);
        assert_eq!(scratch.library().markup(&book.id).unwrap(), expected);
    }

    #[test]
    fn every_highlight_gets_its_own_id_and_the_markup_lists_them_oldest_first() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let book = import_new(&library, &scratch.file("a.pdf", PDF));

        let later = highlight(&library, &book.id, "later", 2_000);
        let earlier = highlight(&library, &book.id, "earlier", 1_000);

        assert_ne!(earlier.id, later.id);
        assert_eq!(
            library.markup(&book.id).unwrap().highlights,
            vec![earlier, later]
        );
    }

    #[test]
    fn markup_and_reading_progress_are_kept_side_by_side() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let book = import_new(&library, &scratch.file("a.pdf", PDF));

        library
            .save_progress(&book.id, &progress(7, 1_000), by_date())
            .unwrap();
        highlight(&library, &book.id, "the passage", 1_000);
        library
            .save_progress(&book.id, &progress(8, 2_000), by_date())
            .unwrap();

        assert_eq!(
            listed_progress(&library, &book.id),
            Some(progress(8, 2_000))
        );
        assert_eq!(library.markup(&book.id).unwrap().highlights.len(), 1);
    }

    #[test]
    fn delete_takes_the_markup_along() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let source = scratch.file("a.pdf", PDF);
        let book = import_new(&library, &source);
        highlight(&library, &book.id, "the passage", 1_000);

        library.delete(&book.id).unwrap();
        import_new(&library, &source);

        assert_eq!(library.markup(&book.id).unwrap(), Markup::default());
    }

    const GREEN: Rgb = Rgb { r: 0, g: 200, b: 0 };

    #[test]
    fn a_recolored_highlight_keeps_its_color_after_reopening_the_library() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let book = import_new(&library, &scratch.file("a.pdf", PDF));
        let first = highlight(&library, &book.id, "first", 1_000);
        let second = highlight(&library, &book.id, "second", 2_000);

        library
            .set_highlight_color(&book.id, &first.id, GREEN)
            .unwrap();

        let expected = vec![
            Highlight {
                color: GREEN,
                ..first
            },
            second,
        ];
        assert_eq!(library.markup(&book.id).unwrap().highlights, expected);
        let reopened = scratch.library();
        assert_eq!(reopened.markup(&book.id).unwrap().highlights, expected);
    }

    #[test]
    fn a_comment_survives_reopening_the_library_and_emptying_it_removes_it() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let book = import_new(&library, &scratch.file("a.pdf", PDF));
        let added = highlight(&library, &book.id, "first", 1_000);

        library
            .set_highlight_comment(&book.id, &added.id, "my thought")
            .unwrap();

        let reopened = scratch.library();
        let markup = reopened.markup(&book.id).unwrap();
        assert_eq!(markup.highlights[0].comment.as_deref(), Some("my thought"));

        reopened
            .set_highlight_comment(&book.id, &added.id, "")
            .unwrap();
        assert_eq!(
            scratch.library().markup(&book.id).unwrap().highlights,
            vec![added]
        );
    }

    #[test]
    fn removing_a_highlight_removes_its_comment() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let book = import_new(&library, &scratch.file("a.pdf", PDF));
        let added = highlight(&library, &book.id, "first", 1_000);
        library
            .set_highlight_comment(&book.id, &added.id, "x")
            .unwrap();

        library.remove_highlight(&book.id, &added.id).unwrap();

        assert_eq!(library.markup(&book.id).unwrap(), Markup::default());
    }

    #[test]
    fn a_removed_highlight_is_gone_after_reopening_the_library() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let book = import_new(&library, &scratch.file("a.pdf", PDF));
        let first = highlight(&library, &book.id, "first", 1_000);
        let second = highlight(&library, &book.id, "second", 2_000);

        library.remove_highlight(&book.id, &first.id).unwrap();

        let reopened = scratch.library();
        assert_eq!(reopened.markup(&book.id).unwrap().highlights, vec![second]);
    }

    fn pdf_spot(page_index: u32, x: f64, y: f64) -> Place {
        Place::Pdf { page_index, x, y }
    }

    #[test]
    fn a_page_comment_survives_reopening_the_library_and_emptying_it_removes_it() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let book = import_new(&library, &scratch.file("a.pdf", PDF));

        let added = library
            .add_page_comment(&book.id, pdf_spot(4, 100.5, 200.5), 1_000)
            .unwrap()
            .unwrap();
        assert_eq!(library.markup(&book.id).unwrap(), Markup::default());
        library
            .set_page_comment_text(&book.id, &added.id, "my thought")
            .unwrap();

        let reopened = scratch.library();
        let expected = PageComment {
            text: "my thought".to_owned(),
            ..added.clone()
        };
        assert_eq!(
            reopened.markup(&book.id).unwrap().page_comments,
            vec![expected]
        );
        assert_eq!(added.place, pdf_spot(4, 100.5, 200.5));
        assert_eq!(added.created, 1_000);

        reopened
            .set_page_comment_text(&book.id, &added.id, "")
            .unwrap();
        assert_eq!(
            scratch.library().markup(&book.id).unwrap(),
            Markup::default()
        );
    }

    #[test]
    fn an_epub_page_comment_is_stored_with_its_spot_and_survives_reopening() {
        let scratch = Scratch::new();
        let (library, book) = moby(&scratch);
        let spot = Place::Epub {
            spine_index: 1,
            offset: 5,
        };

        let added = library
            .add_page_comment(&book.id, spot, 1_000)
            .unwrap()
            .unwrap();
        assert_eq!(added.place, spot);
        library
            .set_page_comment_text(&book.id, &added.id, "years")
            .unwrap();

        let expected = PageComment {
            text: "years".to_owned(),
            ..added
        };
        assert_eq!(
            scratch.library().markup(&book.id).unwrap().page_comments,
            vec![expected]
        );
    }

    #[test]
    fn a_removed_page_comment_is_gone_after_reopening_the_library() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let book = import_new(&library, &scratch.file("a.pdf", PDF));
        let first = library
            .add_page_comment(&book.id, pdf_spot(0, 1.0, 2.0), 1)
            .unwrap()
            .unwrap();
        let second = library
            .add_page_comment(&book.id, pdf_spot(1, 3.0, 4.0), 2)
            .unwrap()
            .unwrap();
        for comment in [&first, &second] {
            library
                .set_page_comment_text(&book.id, &comment.id, "x")
                .unwrap();
        }

        library.remove_page_comment(&book.id, &first.id).unwrap();

        let ids: Vec<_> = scratch
            .library()
            .markup(&book.id)
            .unwrap()
            .page_comments
            .into_iter()
            .map(|n| n.id)
            .collect();
        assert_eq!(ids, [second.id]);
    }

    #[test]
    fn changing_or_removing_an_unknown_page_comment_does_nothing() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let book = import_new(&library, &scratch.file("a.pdf", PDF));
        let added = library
            .add_page_comment(&book.id, pdf_spot(0, 1.0, 2.0), 1)
            .unwrap()
            .unwrap();
        library
            .set_page_comment_text(&book.id, &added.id, "x")
            .unwrap();

        library
            .set_page_comment_text(&book.id, "nope", "y")
            .unwrap();
        library.remove_page_comment(&book.id, "nope").unwrap();

        assert_eq!(library.markup(&book.id).unwrap().page_comments.len(), 1);
    }

    #[test]
    fn delete_takes_page_comments_along() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let book = import_new(&library, &scratch.file("a.pdf", PDF));
        let added = library
            .add_page_comment(&book.id, pdf_spot(0, 1.0, 2.0), 1)
            .unwrap()
            .unwrap();
        library
            .set_page_comment_text(&book.id, &added.id, "x")
            .unwrap();

        library.delete(&book.id).unwrap();

        assert_eq!(library.markup(&book.id).unwrap(), Markup::default());
    }

    #[test]
    fn changing_or_removing_an_unknown_highlight_does_nothing() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let book = import_new(&library, &scratch.file("a.pdf", PDF));
        let added = highlight(&library, &book.id, "first", 1_000);

        library
            .set_highlight_color(&book.id, "nope", GREEN)
            .unwrap();
        library
            .set_highlight_comment(&book.id, "nope", "x")
            .unwrap();
        library.remove_highlight(&book.id, "nope").unwrap();

        assert_eq!(library.markup(&book.id).unwrap().highlights, vec![added]);
    }

    #[test]
    fn editing_highlights_of_a_book_that_is_not_there_does_nothing() {
        let scratch = Scratch::new();
        let library = scratch.library();
        library.set_highlight_color(NO_BOOK, "a", GREEN).unwrap();
        library.set_highlight_comment(NO_BOOK, "a", "x").unwrap();
        library.remove_highlight(NO_BOOK, "a").unwrap();
        assert!(!library.folder.state_file(NO_BOOK).exists());
    }

    #[test]
    fn editing_highlights_refuses_an_id_that_points_outside_the_library() {
        let scratch = Scratch::new();
        let library = scratch.library();
        for id in ["../x", "/tmp", "", ".staging"] {
            let err = library.set_highlight_color(id, "a", GREEN).unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::InvalidInput, "{id:?}");
            let err = library.set_highlight_comment(id, "a", "x").unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::InvalidInput, "{id:?}");
            let err = library.remove_highlight(id, "a").unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::InvalidInput, "{id:?}");
        }
    }

    #[test]
    fn highlighting_in_a_book_that_is_not_there_adds_nothing() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let added = library
            .add_highlight(NO_BOOK, anchor(), "x", YELLOW, 1)
            .unwrap();
        assert_eq!(added, None);
        assert_eq!(library.markup(NO_BOOK).unwrap(), Markup::default());
        assert!(!library.folder.state_file(NO_BOOK).exists());
    }

    #[test]
    fn adding_a_page_comment_to_a_deleted_book_adds_nothing() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let book = import_new(&library, &scratch.file("a.pdf", PDF));
        library.delete(&book.id).unwrap();

        let added = library
            .add_page_comment(&book.id, pdf_spot(0, 1.0, 2.0), 1)
            .unwrap();

        assert_eq!(added, None);
        assert!(!library.folder.state_file(&book.id).exists());
        assert!(!library.folder.details(&book.id).exists());
    }

    #[test]
    fn markup_refuses_an_id_that_points_outside_the_library() {
        let scratch = Scratch::new();
        let library = scratch.library();
        for id in ["../x", "/tmp", "", ".staging"] {
            let err = library.markup(id).unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::InvalidInput, "{id:?}");
            let err = library
                .add_highlight(id, anchor(), "x", YELLOW, 1)
                .unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::InvalidInput, "{id:?}");
        }
    }

    #[test]
    fn a_corrupt_state_makes_its_file_a_new_book_without_markup() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let book = import_new(&library, &scratch.file("a.pdf", PDF));
        highlight(&library, &book.id, "the passage", 1_000);
        fs::write(state_path(&library, &book), b"garbage").unwrap();

        let listed = library.books().unwrap();

        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].path, book.path);
        assert_ne!(listed[0].id, book.id);
        assert_eq!(library.markup(&book.id).unwrap(), Markup::default());
        assert_eq!(library.markup(&listed[0].id).unwrap(), Markup::default());
    }

    #[test]
    fn the_highlight_colors_are_the_six_built_in_ones_yellow_first() {
        let scratch = Scratch::new();
        let names: Vec<_> = scratch
            .library()
            .highlight_colors()
            .colors
            .into_iter()
            .map(|c| c.id)
            .collect();
        assert_eq!(names, ["Yellow", "Green", "Blue", "Pink", "Purple", "Red"]);
    }

    #[test]
    fn the_default_highlight_color_is_yellow_until_set() {
        let scratch = Scratch::new();
        let library = scratch.library();
        assert_eq!(library.highlight_colors().default_color.id, "Yellow");
    }

    #[test]
    fn the_default_highlight_color_survives_reopening_the_library() {
        let scratch = Scratch::new();
        let library = scratch.library();

        library.set_default_highlight_color("Purple").unwrap();
        assert_eq!(library.highlight_colors().default_color.id, "Purple");
        drop(library);

        let library = scratch.library();
        let HighlightColors {
            colors,
            default_color: purple,
        } = library.highlight_colors();
        assert_eq!(purple.id, "Purple");
        assert_eq!(purple, colors[4]);
        library.set_default_highlight_color("Yellow").unwrap();
        assert_eq!(
            scratch.library().highlight_colors().default_color.id,
            "Yellow"
        );
    }

    #[test]
    fn setting_an_unknown_default_highlight_color_is_invalid_input() {
        let scratch = Scratch::new();
        let library = scratch.library();
        library.set_default_highlight_color("Green").unwrap();

        let err = library.set_default_highlight_color("Teal").unwrap_err();

        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        assert_eq!(library.highlight_colors().default_color.id, "Green");
    }

    const TEAL: Rgb = Rgb {
        r: 0,
        g: 128,
        b: 128,
    };

    #[test]
    fn a_custom_color_is_listed_after_the_built_in_ones_and_survives_reopening() {
        let scratch = Scratch::new();
        let library = scratch.library();

        let teal = library.add_custom_color(TEAL, 100).unwrap();
        let brown = library
            .add_custom_color(
                Rgb {
                    r: 150,
                    g: 75,
                    b: 0,
                },
                200,
            )
            .unwrap();

        assert!(teal.is_custom && teal.rgb == TEAL);
        drop(library);
        assert_eq!(
            scratch.library().highlight_colors().colors[6..],
            [teal, brown]
        );
    }

    #[test]
    fn there_is_no_limit_to_custom_colors() {
        let scratch = Scratch::new();
        let library = scratch.library();
        for n in 0..50 {
            library
                .add_custom_color(Rgb { r: n, g: 0, b: 0 }, 1)
                .unwrap();
        }
        assert_eq!(library.highlight_colors().colors.len(), 6 + 50);
    }

    #[test]
    fn a_custom_color_can_be_the_default_highlight_color_and_survives_reopening() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let teal = library.add_custom_color(TEAL, 100).unwrap();

        library.set_default_highlight_color(&teal.id).unwrap();

        assert_eq!(scratch.library().highlight_colors().default_color, teal);
    }

    #[test]
    fn removing_a_custom_color_makes_yellow_the_default_again() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let teal = library.add_custom_color(TEAL, 100).unwrap();
        library.set_default_highlight_color(&teal.id).unwrap();

        library.remove_custom_color(&teal.id).unwrap();

        let after = library.highlight_colors();
        assert_eq!(after.colors.len(), 6);
        assert_eq!(after.default_color.id, "Yellow");
        let err = library.set_default_highlight_color(&teal.id).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn built_in_colors_cannot_be_removed() {
        let scratch = Scratch::new();
        let library = scratch.library();

        library.remove_custom_color("Yellow").unwrap();

        assert_eq!(library.highlight_colors().colors.len(), 6);
        assert_eq!(library.highlight_colors().colors[0].id, "Yellow");
    }

    #[test]
    fn a_highlight_keeps_its_color_when_the_custom_color_is_removed() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let book = import_new(&library, &scratch.file("a.pdf", PDF));
        let teal = library.add_custom_color(TEAL, 100).unwrap();
        let added = library
            .add_highlight(&book.id, Anchor::Pdf(vec![]), "x", teal.rgb, 1)
            .unwrap()
            .unwrap();

        library.remove_custom_color(&teal.id).unwrap();

        assert_eq!(library.markup(&book.id).unwrap().highlights, vec![added]);
    }

    #[test]
    fn corrupt_preferences_read_as_yellow_and_are_replaced_by_the_next_choice() {
        let scratch = Scratch::new();
        let library = scratch.library();
        fs::write(library.folder.preferences(), b"garbage").unwrap();
        let corrupt = library.highlight_colors();
        assert_eq!(corrupt.default_color.id, "Yellow");
        assert_eq!(corrupt.colors.len(), 6);

        library.set_default_highlight_color("Red").unwrap();

        assert_eq!(library.highlight_colors().default_color.id, "Red");
    }

    #[test]
    fn the_highlight_colors_and_the_default_come_together() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let teal = library.add_custom_color(TEAL, 100).unwrap();
        library.set_default_highlight_color(&teal.id).unwrap();

        let both = library.highlight_colors();
        assert_eq!(both.colors.len(), 7);
        assert_eq!(both.colors[6], teal);
        assert_eq!(both.default_color, teal);

        library.remove_custom_color(&teal.id).unwrap();
        let both = library.highlight_colors();
        assert_eq!(both.colors.len(), 6);
        assert_eq!(both.default_color.id, "Yellow");
    }

    #[test]
    fn preferences_are_a_hidden_file_in_the_state_folder_and_no_book() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let book = import_new(&library, &scratch.file("a.pdf", PDF));

        library.set_default_highlight_color("Blue").unwrap();

        assert!(scratch.0.join("library/.state/preferences.json").is_file());
        assert_eq!(library.books().unwrap(), vec![book]);
        assert_eq!(
            fs::read_dir(scratch.0.join("library/.staging"))
                .unwrap()
                .count(),
            0
        );
    }

    #[test]
    fn a_re_imported_book_is_the_newest_again() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let source = scratch.file("a.pdf", b"%PDF-first");
        let first = import_new(&library, &source);
        sleep(Duration::from_millis(5));
        let second = import_new(&library, &scratch.file("b.pdf", b"%PDF-second"));
        sleep(Duration::from_millis(5));
        library.delete(&first.id).unwrap();

        let again = import_new(&library, &source);

        assert_eq!(library.books().unwrap(), vec![again, second]);
    }

    #[test]
    fn imports_on_several_threads_are_all_listed() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let sources: Vec<_> = (0..8)
            .map(|n| scratch.file(&format!("{n}.pdf"), format!("%PDF-book {n}").as_bytes()))
            .collect();

        std::thread::scope(|scope| {
            for source in &sources {
                scope.spawn(|| import_new(&library, source));
            }
        });

        assert_eq!(library.books().unwrap().len(), 8);
        assert_eq!(scratch.library().books().unwrap().len(), 8);
    }

    /// Imports `count` distinct Books and saves Reading Progress for each.
    fn library_of_opened_books(scratch: &Scratch, count: u32) -> Library {
        let library = scratch.library();
        for n in 0..count {
            let source = scratch.file("source.pdf", format!("%PDF-book {n}").as_bytes());
            let book = import_new(&library, &source);
            library
                .save_progress(&book.id, &progress(n, u64::from(n) + 1), by_date())
                .unwrap();
        }
        library
    }

    #[test]
    fn saving_reading_progress_keeps_no_history() {
        let scratch = Scratch::new();
        let library = scratch.library();
        let book = import_new(&library, &scratch.file("a.pdf", PDF));

        for n in 0..1_000 {
            library
                .save_progress(&book.id, &progress(n % 300, u64::from(n) + 1), by_date())
                .unwrap();
        }

        let stored = fs::read(state_path(&library, &book)).unwrap();
        assert!(stored.len() < 400, "state is {} bytes", stored.len());
        assert_eq!(
            listed_progress(&library, &book.id),
            Some(progress(999 % 300, 1_000))
        );
    }

    /// Debug timings mean nothing: run with
    /// `cargo test --release -- --ignored listing_1000`.
    #[test]
    #[ignore = "timing budget; run in a release build"]
    fn listing_1000_books_with_reading_progress_takes_under_100_ms() {
        let scratch = Scratch::new();
        let library = library_of_opened_books(&scratch, 1_000);

        let start = std::time::Instant::now();
        let books = library.books().unwrap();
        let elapsed = start.elapsed();

        assert_eq!(books.len(), 1_000);
        assert!(books.iter().all(|d| d.progress.is_some()));
        assert!(elapsed < Duration::from_millis(100), "took {elapsed:?}");
    }
}
