import CoreGraphics
import Foundation
import Observation
import Synchronization

extension Book: Identifiable {}

extension Book {
    /// The stored File. Built without asking the file system whether it is a directory.
    var file: URL { URL(fileURLWithPath: path, isDirectory: false) }
    /// When the Book was first Imported.
    var importedDate: Date { Date(milliseconds: imported) }
    /// Where the user left off, to reopen the Book there; nil if never opened.
    var place: Place? { progress?.place }
    /// When the Reader last showed the Book; nil if never.
    var lastRead: Date? { progress.map { Date(milliseconds: $0.lastRead) } }

    /// Where the Cover of this Book comes from.
    fileprivate func coverSource(core: Library) -> CoverSource {
        switch format {
        case .pdf: .pdf(file)
        case .epub:
            .epub(title: title, author: author) { [id] in (try? core.epubCover(id: id)) ?? nil }
        }
    }

    /// What VoiceOver reads for the Book's cell: the Title, then how far it
    /// has been read if it was opened.
    var accessibilityLabel: String {
        guard let percentRead else { return title }
        return String(localized: "\(title), \(Int(percentRead)) % read")
    }
}

/// How an Import ended.
enum ImportOutcome {
    /// The File is now a new Book in the Library.
    case imported(Book)
    /// The Library already holds this File's content: the existing Book.
    case existing(Book)
    /// The File isn't Imported, and nothing was left behind: why, as the core says.
    case failed(LibraryError)
}

/// A File that wasn't Imported, and why.
struct ImportProblem {
    let file: URL
    let error: LibraryError
}

/// What the window shows after Importing Files.
enum ImportedFiles {
    /// A single File was Imported, or was already in the Library: open its Book.
    case open(Book, problems: [ImportProblem])
    /// Several Files were Imported, or none: nothing opens, the Gallery shows; alert if there are problems.
    case problems([ImportProblem])
}

/// An Import in progress: what the progress modal shows.
struct RunningImport: Equatable {
    /// The file name until the Import is done, then its Title.
    fileprivate(set) var title: String
    /// Share of the File copied, 0...1.
    fileprivate(set) var fraction: Double = 0
}

/// Forwards the core's progress, reported on the Import's thread, to the main actor.
private final class ProgressRelay: ImportProgress {
    private let update: @Sendable @MainActor (Double) -> Void

    init(_ update: @escaping @Sendable @MainActor (Double) -> Void) { self.update = update }

    /// The last whole percent forwarded.
    private let forwarded = Atomic(-1)

    /// Forwards whole percent steps only, not every chunk copied.
    func report(copied: UInt64, total: UInt64) {
        let fraction = total == 0 ? 1 : Double(copied) / Double(total)
        let percent = Int(fraction * 100)
        guard forwarded.exchange(percent, ordering: .relaxed) != percent else { return }
        Task { @MainActor [update] in update(fraction) }
    }
}

extension Date {
    /// The core's time: milliseconds since the Unix epoch.
    init(milliseconds: UInt64) { self.init(timeIntervalSince1970: Double(milliseconds) / 1000) }

    /// This time as the core takes it: whole milliseconds since the Unix epoch, none before.
    var millisecondsSince1970: UInt64 { UInt64(max(0, timeIntervalSince1970 * 1000).rounded()) }
}

/// The Library as the core listed it at one change: the Books, newest Import
/// first, and the core's `Listing` that orders and searches those same Books.
/// One core call makes both, so they can't disagree.
private struct LibraryListing: Sendable {
    private(set) var books: [Book]
    /// Library Search over `books`: positions in them, in Sort order.
    let sorted: Listing

    /// Nil if the core can't list the Library.
    init?(_ core: Library, sort: Sort) {
        guard let listed = try? core.listing(sort: sort) else { return nil }
        sorted = listed.listing
        books = listed.books
    }

    /// Gives Book `id` the saved `progress`, for a save that the core says
    /// changes nothing the Gallery shows, so nothing is listed again.
    mutating func keep(_ progress: ReadingProgress, of id: String) {
        guard let index = books.firstIndex(where: { $0.id == id }) else { return }
        books[index].progress = progress
    }
}

/// The Library as the Books Context sees it: its Books, Imports, Covers and
/// the open Book, on the core's Books folder.
@MainActor @Observable
final class LibraryModel {
    /// Every Book, newest Import first.
    var books: [Book] { listing?.books ?? [] }
    /// How the Gallery orders its Books. Remembered on this Device (user
    /// defaults, not synced); setting it re-orders once the core has the new listing.
    var sort: LibrarySort {
        didSet {
            guard sort != oldValue else { return }
            sort.save(to: defaults)
            Task { await relist { _ in } }
        }
    }
    /// The Book that fills the window, if any. A Book that leaves the Library
    /// meanwhile closes by itself.
    var open: Book? { openID.flatMap(book(id:)) }
    /// Non-nil from the start of an Import until it finishes. The UI is
    /// disabled meanwhile.
    private(set) var runningImport: RunningImport?
    private var openID: String?
    /// The Highlight Colors to choose from: the built-in ones, yellow first, then
    /// the Custom Colors in the order added.
    private(set) var highlightColors: [HighlightColor]
    /// The color of new Highlights; one of `highlightColors`. Yellow until chosen.
    private(set) var defaultHighlightColor: HighlightColor
    /// The Library as the core last listed it, in `sort` order: what the Gallery
    /// shows. Nil only if the core couldn't list it.
    private var listing: LibraryListing?
    @ObservationIgnored private let core: Library
    @ObservationIgnored private let coversFolder: URL
    /// Made per use, so new Covers suit the displays in use (see `CoverStore`).
    private var covers: CoverStore { CoverStore(folder: coversFolder) }
    @ObservationIgnored private let defaults: UserDefaults
    /// The latest `importFiles` call; the next one waits for it.
    @ObservationIgnored private var lastImportFiles: Task<Void, Never>?
    /// The latest access to the core; the next one waits for it.
    @ObservationIgnored private var lastCoreAccess: Task<Void, Never>?
    /// Covers decoded lately, by Book ID; the system evicts them as memory gets tight.
    @ObservationIgnored private let decodedCovers = {
        let cache = NSCache<NSString, CGImage>()
        cache.totalCostLimit = 64 << 20
        return cache
    }()

    /// Shows the Library of `core`, with Covers in `coversFolder` and the sort
    /// choice in `defaults`. Books whose details the core lacks are listed by
    /// file name; the core's background pass then reads them from the PDFs, one
    /// at a time, and tells this model to list again as they arrive.
    init(core: Library, coversFolder: URL, defaults: UserDefaults) {
        self.core = core
        self.coversFolder = coversFolder
        self.defaults = defaults
        sort = LibrarySort(from: defaults)
        let colors = core.highlightColors()
        highlightColors = colors.colors
        defaultHighlightColor = colors.defaultColor
        // Listed at once, so the Gallery opens in order rather than filling in.
        listing = LibraryListing(core, sort: sort.core)
        core.readMissingDetails(
            reader: PDFKitReader(),
            observer: GalleryRelay { [weak self] in
                Task { await self?.relist { _ in } }
            })
    }

    /// The Books a Gallery shows for Library Search `searchText`: those whose
    /// Title, file name or Author contains it, ignoring case and accents, in
    /// `sort` order. Blank `searchText` matches every Book.
    func visibleBooks(matching searchText: String) -> [Book] {
        guard let listing else { return [] }
        return listing.sorted.matching(search: searchText).map { listing.books[Int($0)] }
    }

    /// The Book with `id`, if it is in the Library.
    func book(id: String) -> Book? { books.first { $0.id == id } }

    /// Opens the Book with `id`; nothing happens if it isn't in the Library.
    func openBook(id: String) {
        if book(id: id) != nil { openID = id }
    }

    func closeBook() { openID = nil }

    /// The saved Cover of `book`, loaded and decoded off the main thread, or
    /// nil if there is none. Cheap, so safe to call while scrolling; never renders.
    func savedCover(for book: Book) async -> CGImage? {
        let covers = covers
        let id = book.id
        if let decoded = decodedCovers.object(forKey: id as NSString) { return decoded }
        return keepDecoded(await Task.detached { covers.saved(id) }.value, id: id)
    }

    /// Keeps `cover` of Book `id` in memory, so cells scrolled back into view
    /// don't decode it again.
    private func keepDecoded(_ cover: CGImage?, id: String) -> CGImage? {
        if let cover { decodedCovers.setObject(cover, forKey: id as NSString, cost: cover.bytesPerRow * cover.height) }
        return cover
    }

    private func removeCover(_ id: String) {
        decodedCovers.removeObject(forKey: id as NSString)
        covers.remove(id)
    }

    /// The Cover of `book`: the saved one, or else made off the main thread (a
    /// PDF's page 1, an EPUB's image) and saved. Nil if it can't be rendered;
    /// the Gallery then keeps the placeholder. Cancelling the caller drops a
    /// request whose rendering hasn't started yet, so cells that scrolled away
    /// don't hold up the others.
    func cover(for book: Book) async -> CGImage? {
        guard !Task.isCancelled else { return nil }
        let covers = covers
        let id = book.id
        let source = book.coverSource(core: core)
        let loading = Task.detached(priority: .utility) {
            Task.isCancelled ? nil : covers.cover(id: id, source: source)
        }
        let cover = await withTaskCancellationHandler {
            await loading.value
        } onCancel: {
            loading.cancel()
        }
        return keepDecoded(cover, id: id)
    }

    /// Copies the File at `url` into the Library without blocking the main
    /// thread, publishing `runningImport` meanwhile. The core refuses a File it
    /// can't Import, e.g. a PDF that can't be opened, and nothing stays in the
    /// Library. The Book's details and Cover are saved before this returns.
    func `import`(_ url: URL) async -> ImportOutcome {
        let scoped = url.startAccessingSecurityScopedResource()
        defer { if scoped { url.stopAccessingSecurityScopedResource() } }

        // Disable the UI before the core reads the File: that alone can take a while.
        runningImport = RunningImport(title: url.deletingPathExtension().lastPathComponent)
        defer { runningImport = nil }

        let core = core
        let covers = covers
        let path = url.path
        // Reports arriving after the Import finished find no runningImport and do nothing.
        let relay = ProgressRelay { [weak self] fraction in
            guard let running = self?.runningImport else { return }
            self?.runningImport?.fraction = max(running.fraction, fraction)
        }
        do {
            let (stored, isNew) = try await Task.detached {
                let (stored, isNew) =
                    switch try core.import(source: path, reader: PDFKitReader(), progress: relay) {
                    case .imported(let stored): (stored, true)
                    case .alreadyImported(let stored): (stored, false)
                    }
                // The Cover is made from the File in the Library, PDF or EPUB.
                _ = covers.cover(id: stored.id, source: stored.coverSource(core: core))
                return (stored, isNew)
            }.value
            let id = stored.id
            // Listed with the Book, so the Gallery shows it too.
            guard let listed = await relist(after: { try? $0.book(id: id) }) else {
                return .failed(.Failed(message: "The imported file couldn't be opened."))
            }
            runningImport?.title = listed.title
            return isNew ? .imported(listed) : .existing(listed)
        } catch let error as LibraryError {
            return .failed(error)
        } catch {
            return .failed(.Failed(message: error.localizedDescription))
        }
    }

    /// Imports `urls` one after another, and says what the window shows: the
    /// Book of a single File, new or already in the Library, opens; several
    /// Files open nothing. A File that isn't Imported is a problem. A call
    /// made while an Import runs waits until it has finished.
    func importFiles(_ urls: [URL]) async -> ImportedFiles {
        let previous = lastImportFiles
        let running = Task { @MainActor in
            await previous?.value
            return await importQueued(urls)
        }
        lastImportFiles = Task { _ = await running.value }
        return await running.value
    }

    private func importQueued(_ urls: [URL]) async -> ImportedFiles {
        var opening: Book?
        var problems: [ImportProblem] = []
        for url in urls {
            switch await `import`(url) {
            case .imported(let book), .existing(let book): opening = book
            case .failed(let error): problems.append(ImportProblem(file: url, error: error))
            }
        }
        guard urls.count == 1, let opening else { return .problems(problems) }
        return .open(opening, problems: problems)
    }

    /// Deletes the Book, its File, its state and its Cover. If that fails,
    /// the Book stays in `books`.
    func delete(_ book: Book) async {
        let id = book.id
        await relist { try? $0.delete(id: id) }
    }

    /// EPUB `book`, opened for reading off the main thread. Nil for a PDF, and
    /// for an EPUB the core can't read.
    func epub(of book: Book) async -> EpubBook? {
        let core = core
        let id = book.id
        return await Task.detached { (try? core.openEpub(id: id)) ?? nil }.value
    }

    /// Saves `place` as Reading Progress of `book`, read `lastRead`. The
    /// core writes off the main thread, in order; this returns when it has, with
    /// the Document updated in every window. The Library is listed again only if
    /// the core says the Gallery shows something new. Does nothing for a
    /// Document that was Deleted. A failed write is ignored: the next save
    /// replaces it.
    func saveProgress(_ place: Place, of book: Book, at lastRead: Date = .now) async {
        let progress = ReadingProgress(place: place, lastRead: lastRead.millisecondsSince1970)
        let id = book.id
        let sort = sort.core
        let changes = await inOrder { try? $0.saveProgress(id: id, progress: progress, sort: sort) }
        switch changes {
        case true?: await relist { _ in }
        case false?: listing?.keep(progress, of: id)
        case nil: break
        }
    }

    /// The Highlights and page Comments of `book`, each oldest first, read at
    /// once. Empty if there are none or the Document was Deleted.
    func markup(of book: Book) async -> (highlights: [Highlight], pageComments: [PageComment]) {
        let id = book.id
        let markup = await inOrder { try? $0.markup(id: id) }
        return (markup?.highlights ?? [], markup?.pageComments ?? [])
    }

    /// Adds a Highlight over `anchor` in the Default Highlight Color to `book`,
    /// `created` now. The core writes off the main thread, in order, and this
    /// returns the stored Highlight when it has. Nil for a Document that was
    /// Deleted, or if the write failed.
    func addHighlight(
        _ anchor: Anchor, quotedText: String, to book: Book, at created: Date = .now
    ) async -> Highlight? {
        let id = book.id
        let rgb = defaultHighlightColor.rgb
        let milliseconds = created.millisecondsSince1970
        return await inOrder {
            try? $0.addHighlight(id: id, anchor: anchor, quotedText: quotedText, color: rgb, created: milliseconds)
        }
    }

    /// Sets the color of `highlight` of `book`. The core writes off the
    /// main thread, in order, and this returns when it has. Does nothing for a
    /// Document that was Deleted.
    func setHighlightColor(_ color: RGB, of highlight: Highlight, in book: Book) async {
        let id = book.id
        let highlightID = highlight.id
        let rgb = Rgb(color)
        await inOrder { try? $0.setHighlightColor(id: id, highlight: highlightID, color: rgb) }
    }

    /// Sets the Comment of `highlight` of `book` to `comment`; empty removes it. The
    /// core writes off the main thread, in order, and this returns when it has.
    /// Does nothing for a Document that was Deleted.
    func setComment(_ comment: String, of highlight: Highlight, in book: Book) async {
        let id = book.id
        let highlightID = highlight.id
        await inOrder { try? $0.setHighlightComment(id: id, highlight: highlightID, comment: comment) }
    }

    /// Deletes `highlight` from `book`. The core writes off the main
    /// thread, in order, and this returns when it has. Does nothing for a
    /// Document that was Deleted.
    func removeHighlight(_ highlight: Highlight, from book: Book) async {
        let id = book.id
        let highlightID = highlight.id
        await inOrder { try? $0.removeHighlight(id: id, highlight: highlightID) }
    }

    /// Adds a page Comment without text at `place` to `book`, `created` now. The
    /// core writes off the main thread, in order, and this returns the stored
    /// page Comment when it has. It isn't listed by `markup(of:)` until it has
    /// text (`setText`). Nil for a Document that was Deleted, or if the write failed.
    func addPageComment(at place: Place, to book: Book, at created: Date = .now) async -> PageComment? {
        let id = book.id
        let milliseconds = created.millisecondsSince1970
        return await inOrder { try? $0.addPageComment(id: id, place: place, created: milliseconds) }
    }

    /// Sets the text of `comment` of `book` to `text`; empty removes the page
    /// Comment. The core writes off the main thread, in order, and this returns when
    /// it has. Does nothing for a Document that was Deleted.
    func setText(_ text: String, of comment: PageComment, in book: Book) async {
        let id = book.id
        let commentID = comment.id
        await inOrder { try? $0.setPageCommentText(id: id, comment: commentID, text: text) }
    }

    /// Deletes the page Comment `comment` from `book`. The core writes off the main
    /// thread, in order, and this returns when it has. Does nothing for a
    /// Document that was Deleted.
    func removePageComment(_ comment: PageComment, from book: Book) async {
        let id = book.id
        let commentID = comment.id
        await inOrder { try? $0.removePageComment(id: id, comment: commentID) }
    }

    /// Makes `color`, one of `highlightColors`, the Default Highlight Color. It
    /// shows at once; the core writes off the main thread, in order, and this
    /// returns when it has.
    func setDefaultHighlightColor(_ color: HighlightColor) async {
        defaultHighlightColor = color
        let id = color.id
        await inOrder { try? $0.setDefaultHighlightColor(id: id) }
    }

    /// Adds `color` as a Custom Color after the others, `added` now. It shows at
    /// once, once the core, which writes off the main thread, in order, has
    /// stored it. Nil if that failed.
    @discardableResult
    func addCustomColor(_ color: RGB, at added: Date = .now) async -> HighlightColor? {
        let rgb = Rgb(color)
        let milliseconds = added.millisecondsSince1970
        guard let stored = await inOrder({ try? $0.addCustomColor(rgb: rgb, added: milliseconds) })
        else { return nil }
        highlightColors.append(stored)
        return stored
    }

    /// Deletes the Custom Color `color`. It goes at once; Highlights keep their
    /// color, and if it was the Default Highlight Color, yellow is again. Does
    /// nothing for a built-in color. The core writes off the main thread, in
    /// order, and this returns when it has.
    func removeCustomColor(_ color: HighlightColor) async {
        guard color.isCustom else { return }
        highlightColors.removeAll { $0 == color }
        if defaultHighlightColor == color { defaultHighlightColor = highlightColors[0] }
        let id = color.id
        await inOrder { try? $0.removeCustomColor(id: id) }
    }

    /// Runs `access` on the core off the main thread, after every earlier access

    /// Runs `access` on the core off the main thread, after every earlier
    /// access has finished, so reads and writes never overlap.
    private func inOrder<T: Sendable>(_ access: @escaping @Sendable (Library) -> T) async -> T {
        let previous = lastCoreAccess
        let core = core
        let running = Task.detached {
            await previous?.value
            return access(core)
        }
        lastCoreAccess = Task { _ = await running.value }
        return await running.value
    }

    /// Changes the Library in the core, off the main thread and in order with
    /// its other changes, then shows the Library as the core lists it after
    /// `change`. Each listing includes every change before it and is shown as
    /// it arrives, so the last one shown is the newest. Returns what `change` returned.
    @discardableResult
    private func relist<T: Sendable>(after change: @escaping @Sendable (Library) -> T) async -> T {
        let sort = sort.core
        let (result, listed) = await inOrder { core in (change(core), LibraryListing(core, sort: sort)) }
        guard let listed else { return result }
        // Gone from the Library: so is its Cover.
        let listedIDs = Set(listed.books.map(\.id))
        for gone in books where !listedIDs.contains(gone.id) { removeCover(gone.id) }
        listing = listed
        return result
    }
}

/// Forwards the core's details pass, reported on its thread, to the main
/// actor: the Gallery lists again as the core reads details.
private final class GalleryRelay: GalleryObserver {
    private let onChanged: @Sendable @MainActor () -> Void

    init(_ onChanged: @escaping @Sendable @MainActor () -> Void) { self.onChanged = onChanged }

    func galleryChanged() {
        Task { @MainActor [onChanged] in onChanged() }
    }
}
