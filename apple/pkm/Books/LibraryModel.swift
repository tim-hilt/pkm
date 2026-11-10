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
}

/// The Library as the core listed it at one change: the Books, newest Import
/// first, and the core's `Listing` that orders and searches those same Books.
/// One core call makes both, so they can't disagree.
private struct LibraryListing: Sendable {
    let books: [Book]
    /// Library Search over `books`: positions in them, in Sort order.
    let sorted: Listing

    /// Nil if the core can't list the Library.
    init?(_ core: Library, sort: Sort) {
        guard let listed = try? core.listing(sort: sort) else { return nil }
        sorted = listed.listing
        books = listed.books
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
