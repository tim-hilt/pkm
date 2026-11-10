import Foundation
import Testing
import UniformTypeIdentifiers

@testable import pkm

@MainActor @Suite struct LibraryModelTests {
    let root = FileManager.default.temporaryDirectory
        .appending(path: UUID().uuidString, directoryHint: .isDirectory)
    var coversFolder: URL { root.deletingLastPathComponent().appending(path: "\(root.lastPathComponent)-covers") }

    /// This device's user defaults, as far as the Library is concerned: one
    /// suite per test, so tests don't see each other's sort choice.
    var defaults: UserDefaults { UserDefaults(suiteName: root.lastPathComponent)! }

    /// Opens the Library at `root`, keeping its Covers in `coversFolder` and its
    /// sort choice in `defaults`.
    func openLibrary() throws -> LibraryModel {
        LibraryModel(core: try Library.open(root: root.path), coversFolder: coversFolder, defaults: defaults)
    }

    /// Imports a PDF with the given Title, oldest first when called in sequence.
    func importPDF(titled title: String, into library: LibraryModel) async throws {
        _ = await library.import(try writePDF(named: "\(UUID().uuidString).pdf", metadataTitle: title))
        try await Task.sleep(for: .milliseconds(20))
    }

    /// Imports `source` into the Library at `root` without details, the way Sync
    /// leaves a PDF: details are never synced. A Library opened next lists it by
    /// file name until its background pass reads them. Call it before `openLibrary`.
    func importWithoutDetails(_ source: URL) throws {
        let core = try Library.open(root: root.path)
        _ = try core.import(source: source.path, reader: PDFKitReader(), progress: NoProgress())
        for book in try core.listing(sort: Sort(key: .dateImported, reversed: false)).books {
            // Import kept the details here; a Device that got the File by Sync has none.
            try? FileManager.default.removeItem(at: root.appending(path: ".details/\(book.id)"))
        }
    }

    private final class NoProgress: ImportProgress {
        func report(copied: UInt64, total: UInt64) {}
    }

    /// The saved Cover files.
    func savedCovers() -> [URL] {
        (try? FileManager.default.contentsOfDirectory(at: coversFolder, includingPropertiesForKeys: nil)) ?? []
    }

    @Test func startsEmpty() throws {
        let library = try openLibrary()
        #expect(library.books.isEmpty)
    }

    @Test func importOpensTheNewBookAndListsIt() async throws {
        let library = try openLibrary()
        let source = try writePDF(named: "scan0042.pdf", pageCount: 2)

        let outcome = await library.import(source)

        guard case .imported(let book) = outcome else {
            Issue.record("expected .imported, got \(outcome)")
            return
        }
        #expect(library.books == [book])
    }

    @Test func aBookWithoutStoredTitleShowsItsOriginalFileName() async throws {
        let library = try openLibrary()

        _ = await library.import(try writePDF(named: "scan0042.pdf"))

        #expect(library.books.first?.title == "scan0042")
    }

    @Test func listsTheNewestImportFirst() async throws {
        let library = try openLibrary()
        _ = await library.import(try writePDF(named: "old.pdf", metadataTitle: "Old"))
        try await Task.sleep(for: .milliseconds(20))
        _ = await library.import(try writePDF(named: "new.pdf", metadataTitle: "New"))

        #expect(library.books.map(\.title) == ["New", "Old"])
    }

    @Test func importingTheSameContentAgainIsADuplicateOfTheExistingBook() async throws {
        let library = try openLibrary()
        let source = try writePDF(named: "a.pdf")
        guard case .imported(let first) = await library.import(source) else {
            Issue.record("expected .imported")
            return
        }
        // Generated PDFs differ on every write, so copy the same bytes under another name.
        let renamed = source.deletingLastPathComponent().appending(path: "renamed.pdf")
        try FileManager.default.copyItem(at: source, to: renamed)

        let outcome = await library.import(renamed)

        guard case .existing(let existing) = outcome else {
            Issue.record("expected .existing, got \(outcome)")
            return
        }
        #expect(existing == first)
        #expect(library.books == [first])
    }

    // MARK: Importing several Files

    @Test func oneNewFileOpensItsBook() async throws {
        let library = try openLibrary()

        let result = await library.importFiles([try writePDF(named: "a.pdf")])

        guard case .open(let book, let problems) = result else {
            Issue.record("expected .open, got \(result)")
            return
        }
        #expect([book] == library.books)
        #expect(problems.isEmpty)
    }

    @Test func oneRejectedFileIsReportedAsAProblemAndNothingIsCopied() async throws {
        let library = try openLibrary()
        let garbage = try writeFile(named: "notes.pdf", bytes: Data("just some text".utf8))

        let result = await library.importFiles([garbage])

        guard case .problems(let problems) = result, problems.count == 1 else {
            Issue.record("expected one problem, got \(result)")
            return
        }
        #expect(problems[0].file == garbage)
        guard case .UnsupportedFormat = problems[0].error else {
            Issue.record("expected .UnsupportedFormat, got \(problems[0].error)")
            return
        }
        #expect(library.books.isEmpty)
    }

    @Test func oneFailedFileIsReportedAsAProblemWithTheSystemsReason() async throws {
        let library = try openLibrary()
        let missing = root.appending(path: "missing.pdf")

        let result = await library.importFiles([missing])

        guard case .problems(let problems) = result, problems.count == 1,
            case .Failed(let message) = problems[0].error
        else {
            Issue.record("expected one failed problem, got \(result)")
            return
        }
        #expect(message.localizedCaseInsensitiveContains("no such file"))
    }

    @Test func severalFilesOpenNothingAndListProblems() async throws {
        let library = try openLibrary()
        let good = try writePDF(named: "a.pdf")
        let garbage = try writeFile(named: "notes.pdf", bytes: Data("just some text".utf8))
        let other = try writePDF(named: "b.pdf", metadataTitle: "Other")

        let result = await library.importFiles([good, garbage, other])

        guard case .problems(let problems) = result else {
            Issue.record("expected .problems, got \(result)")
            return
        }
        #expect(problems.map(\.file) == [garbage])
        #expect(library.books.count == 2)
    }

    @Test func severalNewFilesAreImportedAndNothingOpens() async throws {
        let library = try openLibrary()
        let files = try (1...3).map { try writePDF(named: "\($0).pdf", metadataTitle: "Book \($0)") }

        let result = await library.importFiles(files)

        guard case .problems(let problems) = result else {
            Issue.record("expected .problems, got \(result)")
            return
        }
        #expect(library.books.map(\.title).sorted() == ["Book 1", "Book 2", "Book 3"])
        #expect(problems.isEmpty)
    }

    @Test func oneFileAlreadyInTheLibraryOpensTheExistingBook() async throws {
        let library = try openLibrary()
        let source = try writePDF(named: "a.pdf")
        guard case .open(let existing, _) = await library.importFiles([source]) else { throw ImportFailed() }
        let renamed = source.deletingLastPathComponent().appending(path: "renamed.pdf")
        try FileManager.default.copyItem(at: source, to: renamed)

        let result = await library.importFiles([renamed])

        guard case .open(let book, let problems) = result else {
            Issue.record("expected .open, got \(result)")
            return
        }
        #expect(book == existing)
        #expect(problems.isEmpty)
        #expect(library.books == [existing])
    }

    @Test func severalFilesWithADuplicateOpenNothing() async throws {
        let library = try openLibrary()
        let existing = try writePDF(named: "a.pdf", metadataTitle: "Existing")
        _ = await library.importFiles([existing])
        let duplicate = existing.deletingLastPathComponent().appending(path: "copy.pdf")
        try FileManager.default.copyItem(at: existing, to: duplicate)
        let new = try writePDF(named: "b.pdf", metadataTitle: "New")

        let result = await library.importFiles([new, duplicate])

        guard case .problems(let problems) = result else {
            Issue.record("expected .problems, got \(result)")
            return
        }
        #expect(problems.isEmpty)
        #expect(library.books.count == 2)
    }

    @Test func severalFilesListOnlyRejectedAndFailedOnesAsProblems() async throws {
        let library = try openLibrary()
        let new = try writePDF(named: "b.pdf", metadataTitle: "New")
        let garbage = try writeFile(named: "notes.pdf", bytes: Data("just some text".utf8))
        let missing = root.appending(path: "missing.pdf")

        let result = await library.importFiles([garbage, new, missing])

        guard case .problems(let problems) = result else {
            Issue.record("expected .problems, got \(result)")
            return
        }
        #expect(library.books.map(\.title) == ["New"])
        #expect(problems.map(\.file) == [garbage, missing])
    }

    @Test func severalFilesThatAllFailOpenNothing() async throws {
        let library = try openLibrary()
        let garbage = try writeFile(named: "notes.pdf", bytes: Data("just some text".utf8))
        let missing = root.appending(path: "missing.pdf")

        let result = await library.importFiles([garbage, missing])

        guard case .problems(let problems) = result else {
            Issue.record("expected .problems, got \(result)")
            return
        }
        #expect(problems.map(\.file) == [garbage, missing])
        #expect(library.books.isEmpty)
    }

    @Test func aCallWhileAnImportRunsWaitsForItAndBothFinish() async throws {
        let library = try openLibrary()
        let big = try writePDF(named: "big.pdf", metadataTitle: "Big", paddedTo: 64_000_000)
        let small = try writePDF(named: "small.pdf", metadataTitle: "Small")
        var finished: [String] = []

        let first = Task { @MainActor in
            _ = await library.importFiles([big])
            finished.append("big")
        }
        while library.runningImport == nil { try await Task.sleep(for: .milliseconds(1)) }
        let second = Task { @MainActor in
            _ = await library.importFiles([small])
            finished.append("small")
        }
        await first.value
        await second.value

        #expect(finished == ["big", "small"])
        #expect(library.books.map(\.title).sorted() == ["Big", "Small"])
    }

    @Test func importingAMissingFileFailsWithTheSystemsReason() async throws {
        let library = try openLibrary()
        let missing = root.appending(path: "missing.pdf")

        let outcome = await library.import(missing)

        guard case .failed(.Failed(let message)) = outcome else {
            Issue.record("expected .failed, got \(outcome)")
            return
        }
        #expect(message.localizedCaseInsensitiveContains("no such file"))
        #expect(library.books.isEmpty)
    }

    @Test func garbageBytesAreRejectedAsNotAPDFAndNothingIsCopied() async throws {
        let library = try openLibrary()
        let garbage = try writeFile(named: "notes.pdf", bytes: Data("just some text".utf8))

        let outcome = await library.import(garbage)

        guard case .failed(.UnsupportedFormat) = outcome else {
            Issue.record("expected .failed(.UnsupportedFormat), got \(outcome)")
            return
        }
        #expect(library.books.isEmpty)
    }

    @Test func aDamagedPDFIsRejectedAsUnreadable() async throws {
        let library = try openLibrary()
        let damaged = try writeFile(named: "damaged.pdf", bytes: Data("%PDF-1.7\nnot really".utf8))

        let outcome = await library.import(damaged)

        guard case .failed(.UnreadablePdf) = outcome else {
            Issue.record("expected .failed(.UnreadablePdf), got \(outcome)")
            return
        }
        #expect(library.books.isEmpty)
    }

    @Test func aPasswordProtectedPDFIsRejectedAsLockedAndNothingIsCopied() async throws {
        let library = try openLibrary()
        let locked = try writePDF(named: "locked.pdf", userPassword: "secret", ownerPassword: "secret")

        let outcome = await library.import(locked)

        guard case .failed(.LockedPdf) = outcome else {
            Issue.record("expected .failed(.LockedPdf), got \(outcome)")
            return
        }
        #expect(library.books.isEmpty)
    }

    @Test func aPDFThatOnlyRestrictsPermissionsImportsNormally() async throws {
        let library = try openLibrary()
        let restricted = try writePDF(named: "restricted.pdf", ownerPassword: "owner")

        let outcome = await library.import(restricted)

        guard case .imported = outcome else {
            Issue.record("expected .imported, got \(outcome)")
            return
        }
        #expect(library.books.count == 1)
    }

    @Test func aDeletedBookDisappearsAndCanBeImportedAgain() async throws {
        let library = try openLibrary()
        let source = try writePDF(named: "a.pdf")
        let other = try writePDF(named: "b.pdf")
        guard case .imported(let doomed) = await library.import(source),
            case .imported(let kept) = await library.import(other)
        else {
            Issue.record("expected .imported")
            return
        }

        await library.delete(doomed)

        #expect(library.books == [kept])
        #expect(try openLibrary().books == [kept])
        guard case .imported(let again) = await library.import(source) else {
            Issue.record("expected the removed File to Import as new")
            return
        }
        #expect(again.id != doomed.id)
        #expect(library.books == [again, kept])
    }

    @Test func reopeningTheLibraryKeepsItsBook() async throws {
        _ = await (try openLibrary()).import(try writePDF(named: "a.pdf"))

        #expect(try openLibrary().books.count == 1)
    }

    @Test func importSavesTheTitleFileNameAndAuthor() async throws {
        let library = try openLibrary()

        _ = await library.import(try writePDF(named: "scan0042.pdf", metadataTitle: "Moby-Dick", author: "Melville"))

        let book = try #require(library.books.first)
        #expect(book.title == "Moby-Dick")
        #expect(book.file.lastPathComponent == "scan0042.pdf")
        #expect(book.author == "Melville")
    }

    @Test func aPDFWithoutTitleOrAuthorShowsItsFileNameAndNoAuthor() async throws {
        let library = try openLibrary()

        _ = await library.import(try writePDF(named: "scan0042.pdf"))

        let book = try #require(library.books.first)
        #expect(book.title == "scan0042")
        #expect(book.author == nil)
    }

    @Test func aBookExposesItsStoredFile() async throws {
        let library = try openLibrary()

        _ = await library.import(try writePDF(named: "a.pdf"))

        let book = try #require(library.books.first)
        #expect(FileManager.default.fileExists(atPath: book.file.path))
        #expect(book.file.lastPathComponent == "a.pdf")
    }

    @Test func titlesAndAuthorsSurviveReopeningTheLibrary() async throws {
        _ = await (try openLibrary()).import(
            try writePDF(named: "a.pdf", metadataTitle: "Moby-Dick", author: "Melville"))

        let book = try #require(try openLibrary().books.first)

        #expect(book.title == "Moby-Dick")
        #expect(book.author == "Melville")
    }

    @Test func missingSavedTitlesShowFileNamesFirstAndTitlesAfterTheBackgroundPass() async throws {
        try importWithoutDetails(try writePDF(named: "scan0042.pdf", metadataTitle: "Moby-Dick", author: "Melville"))

        let library = try openLibrary()
        let stale = try #require(library.books.first)
        #expect(stale.title == "scan0042")
        #expect(stale.author == nil)

        try await waitUntil { library.books.first?.title == "Moby-Dick" }
        #expect(library.books.first?.author == "Melville")
        // Views holding the stale value see a change, so their caption updates.
        #expect(library.books.first != stale)
        // Saved, so the next launch has them at once.
        #expect(try openLibrary().books.first?.title == "Moby-Dick")
    }

    @Test func openingTheLibraryOpensNoPDFs() throws {
        // A File PDFKit can't open still lists, by its file name: the Library never looks inside it.
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        try Data("%PDF-1.4 but not a PDF".utf8).write(to: root.appending(path: "broken.pdf"))

        let library = try openLibrary()

        #expect(library.books.map(\.title) == ["broken"])
    }

    @Test func aCoverIsAvailableAfterImportWithoutRendering() async throws {
        let library = try openLibrary()
        guard case .imported(let book) = await library.import(try writePDF(named: "a.pdf")) else {
            Issue.record("expected .imported")
            return
        }
        #expect(savedCovers().count == 1)
        // Without its File, the Cover can only come from the saved copy.
        try FileManager.default.removeItem(at: book.file)

        let cover = try #require(await library.cover(for: book))

        // US Letter, rendered 300 pt wide.
        #expect(abs(Double(cover.height) / Double(cover.width) - 792.0 / 612.0) < 0.01)
        // PDFKit fits the page inside 300 × 388 px at scale 1, so rounding can cost a pixel.
        #expect(cover.width >= 299)
    }

    @Test func aCoverIsRenderedAgainAfterItsSavedCopyIsDeleted() async throws {
        let library = try openLibrary()
        guard case .imported(let book) = await library.import(try writePDF(named: "a.pdf")) else {
            Issue.record("expected .imported")
            return
        }
        try FileManager.default.removeItem(at: coversFolder)

        #expect(await library.cover(for: book) != nil)
        #expect(savedCovers().count == 1)
    }

    @Test func aSavedCoverIsLoadedWithoutRenderingAMissingOne() async throws {
        let library = try openLibrary()
        guard case .imported(let book) = await library.import(try writePDF(named: "a.pdf")) else {
            Issue.record("expected .imported")
            return
        }
        #expect(await library.savedCover(for: book) != nil)

        // Decoded Covers stay in memory, so after a relaunch.
        try FileManager.default.removeItem(at: coversFolder)

        #expect(await (try openLibrary()).savedCover(for: book) == nil)
        #expect(savedCovers().isEmpty)
    }

    @Test func aSavedCoverSurvivesReopeningTheLibrary() async throws {
        guard case .imported(let book) = await (try openLibrary()).import(try writePDF(named: "a.pdf")) else {
            Issue.record("expected .imported")
            return
        }
        try FileManager.default.removeItem(at: book.file)

        #expect(await (try openLibrary()).cover(for: book) != nil)
    }

    @Test func deleteDeletesTheSavedCover() async throws {
        let library = try openLibrary()
        guard case .imported(let book) = await library.import(try writePDF(named: "a.pdf")) else {
            Issue.record("expected .imported")
            return
        }

        await library.delete(book)

        #expect(savedCovers().isEmpty)
    }

    @Test func aRunningImportShowsTheFileNameAndProgressUntilItFinishes() async throws {
        let library = try openLibrary()
        let source = try writePDF(named: "big.pdf", metadataTitle: "Big One", paddedTo: 64_000_000)
        #expect(library.runningImport == nil)

        let importing = Task { @MainActor in _ = await library.import(source) }
        var seen: [RunningImport] = []
        while seen.last?.fraction ?? 0 < 1 {
            if let running = library.runningImport { seen.append(running) }
            if library.runningImport == nil, !seen.isEmpty { break }
            try await Task.sleep(for: .milliseconds(1))
        }
        await importing.value

        #expect(!seen.isEmpty)
        #expect(seen.first?.title == "big")
        #expect(seen.allSatisfy { (0...1).contains($0.fraction) })
        #expect(zip(seen, seen.dropFirst()).allSatisfy { $0.fraction <= $1.fraction })
        #expect(library.runningImport == nil)
    }

    // MARK: Sorting

    /// A Library with Titles imported in this order, so Date Imported is
    /// newest-first: "Zebra", "résumé 10", "Resume 2", "apple".
    func libraryForSorting() async throws -> LibraryModel {
        let library = try openLibrary()
        for title in ["apple", "Resume 2", "résumé 10", "Zebra"] { try await importPDF(titled: title, into: library) }
        return library
    }

    @Test func sortsByDateImportedNewestFirstByDefault() async throws {
        let library = try await libraryForSorting()

        #expect(library.sort == LibrarySort(key: .dateImported, isReversed: false))
        #expect(library.visibleBooks(matching: "").map(\.title) == ["Zebra", "résumé 10", "Resume 2", "apple"])
    }

    @Test func reversingDateImportedListsTheOldestFirst() async throws {
        let library = try await libraryForSorting()

        library.sort.isReversed = true

        try await expectListed(["apple", "Resume 2", "résumé 10", "Zebra"], in: library)
    }

    @Test func sortsByTitleLikeFinder() async throws {
        let library = try await libraryForSorting()

        library.sort.key = .title

        try await expectListed(["apple", "Resume 2", "résumé 10", "Zebra"], in: library)
    }

    @Test func reversingTitleListsZToA() async throws {
        let library = try await libraryForSorting()

        library.sort = LibrarySort(key: .title, isReversed: true)

        try await expectListed(["Zebra", "résumé 10", "Resume 2", "apple"], in: library)
    }

    @Test func aBookWhoseTitleArrivesLaterMovesToItsPlace() async throws {
        try importWithoutDetails(try writePDF(named: "zzz.pdf", metadataTitle: "Alpha"))
        try importWithoutDetails(try writePDF(named: "mmm.pdf", metadataTitle: "Mike"))
        let library = try openLibrary()
        library.sort.key = .title

        // Listed by file name until the background pass reads the Titles.
        try await expectListed(["Alpha", "Mike"], in: library)
    }

    @Test func theSortChoiceIsRememberedOnThisDevice() throws {
        let library = try openLibrary()

        library.sort = LibrarySort(key: .title, isReversed: true)

        #expect(try openLibrary().sort == LibrarySort(key: .title, isReversed: true))
    }

    // MARK: EPUBs

    @Test func anEPUBImportsLikeAPDFWithItsTitleAndAuthorAndTheFileUnchanged() async throws {
        let library = try openLibrary()
        let source = try writeEPUB(named: "book.epub", title: "Emma", author: "Jane Austen")

        guard case .imported(let book) = await library.import(source) else {
            Issue.record("expected .imported")
            return
        }

        #expect(book.format == .epub)
        #expect(book.title == "Emma")
        #expect(book.author == "Jane Austen")
        #expect(book.pageCount == nil)
        #expect(library.books == [book])
        #expect(try Data(contentsOf: book.file) == Data(contentsOf: source))
    }

    @Test func anEPUBWithoutATitleShowsItsFileName() async throws {
        let library = try openLibrary()

        _ = await library.import(try writeEPUB(named: "Walden.epub"))

        #expect(library.books.first?.title == "Walden")
    }

    @Test func aPDFIsStillAPDF() async throws {
        let library = try openLibrary()

        _ = await library.import(try writePDF(named: "a.pdf"))

        #expect(library.books.first?.format == .pdf)
    }

    @Test func anEPUBsCoverIsItsDeclaredImageInOriginalColors() async throws {
        let library = try openLibrary()
        let source = try writeEPUB(
            named: "book.epub",
            manifest: #"<item id="cv" href="cover.png" media-type="image/png" properties="cover-image"/>"#,
            files: ["OEBPS/cover.png": solidPNG(red: 255, green: 0, blue: 0)])
        guard case .imported(let book) = await library.import(source) else {
            Issue.record("expected .imported")
            return
        }
        #expect(savedCovers().count == 1)

        let cover = try #require(await library.savedCover(for: book))

        #expect(cover.height > cover.width)
        let pixel = cover.centerPixel
        #expect(pixel[0] > 245 && pixel[1] < 10 && pixel[2] < 10, "\(pixel)")
    }

    @Test func anEPUBWithoutImagesGetsAPlainCoverMadeAgainWhenItsSavedCopyIsDeleted() async throws {
        let library = try openLibrary()
        let source = try writeEPUB(named: "book.epub", title: "A Long Title Here", author: "Jane Austen")
        guard case .imported(let book) = await library.import(source) else {
            Issue.record("expected .imported")
            return
        }
        #expect(savedCovers().count == 1)
        try FileManager.default.removeItem(at: coversFolder)

        let cover = try #require(await library.cover(for: book))

        // A4 portrait, 300 pt wide, like a PDF's.
        #expect(abs(Double(cover.height) / Double(cover.width) - 1.414) < 0.01)
        #expect(cover.hasInk)
        #expect(savedCovers().count == 1)
    }

    @Test func anEPUBRejectedForDRMOrAFixedLayoutOrAsInvalidLeavesNothingBehind() async throws {
        let library = try openLibrary()
        let encryption = """
            <encryption xmlns:enc="http://www.w3.org/2001/04/xmlenc#"><enc:EncryptedData>\
            <enc:EncryptionMethod Algorithm="http://www.w3.org/2001/04/xmlenc#aes128-cbc"/>\
            </enc:EncryptedData></encryption>
            """
        let drm = try writeEPUB(named: "drm.epub", files: ["META-INF/encryption.xml": Data(encryption.utf8)])
        let fixed = try writeEPUB(
            named: "fixed.epub", metadata: #"<meta property="rendition:layout">pre-paginated</meta>"#)
        let invalid = try writeFile(named: "invalid.epub", bytes: Data("not a zip".utf8))
        let otherZip = try writeFile(named: "other", bytes: zip([("a.txt", Data("a".utf8))]))

        let outcomes =
            [await library.import(drm), await library.import(fixed)]
            + [await library.import(invalid), await library.import(otherZip)]

        guard case .failed(.DrmProtectedEpub) = outcomes[0], case .failed(.FixedLayoutEpub) = outcomes[1],
            case .failed(.InvalidEpub) = outcomes[2], case .failed(.InvalidEpub) = outcomes[3]
        else {
            Issue.record("expected DRM, fixed layout, invalid and invalid, got \(outcomes)")
            return
        }
        #expect(library.books.isEmpty)
        #expect(savedCovers().isEmpty)
    }

    @Test func anEPUBAlreadyInTheLibraryIsADuplicate() async throws {
        let library = try openLibrary()
        let source = try writeEPUB(named: "a.epub", title: "Emma")
        guard case .imported(let first) = await library.import(source) else {
            Issue.record("expected .imported")
            return
        }
        let renamed = source.deletingLastPathComponent().appending(path: "renamed.epub")
        try FileManager.default.copyItem(at: source, to: renamed)

        let outcome = await library.import(renamed)

        guard case .existing(let existing) = outcome else {
            Issue.record("expected .existing, got \(outcome)")
            return
        }
        #expect(existing == first)
    }

    @Test func oneBadEPUBAmongSeveralFilesIsAProblemAndTheOthersOpen() async throws {
        let library = try openLibrary()
        let urls = [
            try writeEPUB(named: "good.epub", title: "Good"),
            try writeFile(named: "bad.epub", bytes: Data("nope".utf8)),
            try writePDF(named: "a.pdf"),
        ]

        let result = await library.importFiles(urls)

        guard case .problems(let problems) = result else {
            Issue.record("expected .problems, got \(result)")
            return
        }
        #expect(Set(library.books.map(\.format)) == [.epub, .pdf])
        #expect(problems.map(\.file.lastPathComponent) == ["bad.epub"])
    }

    @Test func aMixedDropOfAPDFAndAnEPUBImportsBoth() async throws {
        let library = try openLibrary()
        let dropped = DroppedFiles()
        let providers = [
            NSItemProvider(contentsOf: try writePDF(named: "a.pdf"))!,
            NSItemProvider(contentsOf: try writeEPUB(named: "b.epub", title: "Emma"))!,
        ]

        let urls = await dropped.receive(providers)
        let result = await library.importFiles(urls)
        dropped.removeCopies()

        guard case .problems(let problems) = result else {
            Issue.record("expected .problems, got \(result)")
            return
        }
        #expect(problems.isEmpty)
        #expect(library.books.count == 2)
    }

    @Test func searchFindsAnEPUBByItsAuthorAndItSortsLikeAPDF() async throws {
        let library = try openLibrary()
        _ = await library.import(try writePDF(named: "a.pdf", metadataTitle: "Moby-Dick", author: "Melville"))
        _ = await library.import(try writeEPUB(named: "b.epub", title: "Emma", author: "Jane Austen"))

        #expect(library.visibleBooks(matching: "austen").map(\.title) == ["Emma"])
        library.sort.key = .title
        try await expectListed(["Emma", "Moby-Dick"], in: library)
    }

    @Test func deletingAnEPUBDeletesItsCover() async throws {
        let library = try openLibrary()
        guard case .imported(let book) = await library.import(try writeEPUB(named: "a.epub")) else {
            Issue.record("expected .imported")
            return
        }

        await library.delete(book)

        #expect(library.books.isEmpty)
        #expect(savedCovers().isEmpty)
    }

    // MARK: Library Search

    @Test func searchMatchesPartsOfTheTitleTheFileNameAndTheAuthor() async throws {
        let library = try openLibrary()
        _ = await library.import(try writePDF(named: "a.pdf", metadataTitle: "Moby-Dick", author: "Melville"))
        _ = await library.import(try writePDF(named: "scan0042.pdf", metadataTitle: "Walden", author: "Thoreau"))

        #expect(library.visibleBooks(matching: "oby").map(\.title) == ["Moby-Dick"])
        #expect(library.visibleBooks(matching: "can004").map(\.title) == ["Walden"])
        #expect(library.visibleBooks(matching: "melv").map(\.title) == ["Moby-Dick"])
    }

    @Test func searchIgnoresCaseAndAccents() async throws {
        let library = try openLibrary()
        try await importPDF(titled: "Résumé", into: library)
        try await importPDF(titled: "Other", into: library)

        #expect(library.visibleBooks(matching: "RESUME").map(\.title) == ["Résumé"])
    }

    @Test func searchKeepsTheCurrentOrder() async throws {
        let library = try await libraryForSorting()

        #expect(library.visibleBooks(matching: "resume").map(\.title) == ["résumé 10", "Resume 2"])

        library.sort.key = .title
        try await expectListed(["Resume 2", "résumé 10"], matching: "resume", in: library)
    }

    @Test func noMatchesShowNothingAndABlankSearchShowsEverything() async throws {
        let library = try await libraryForSorting()

        #expect(library.visibleBooks(matching: "xyz").isEmpty)
        #expect(library.visibleBooks(matching: " ").count == 4)
    }

    @Test func aBookWhoseAuthorArrivesLaterJoinsTheResults() async throws {
        try importWithoutDetails(try writePDF(named: "a.pdf", metadataTitle: "Moby-Dick", author: "Melville"))
        let library = try openLibrary()
        #expect(library.visibleBooks(matching: "melville").isEmpty)

        try await waitUntil { library.visibleBooks(matching: "melville").map(\.title) == ["Moby-Dick"] }
    }

    /// Imports a new PDF and returns its Book.
    func importBook(into library: LibraryModel) async throws -> Book {
        guard case .imported(let book) = await library.import(try writePDF(named: "\(UUID().uuidString).pdf"))
        else { throw ImportFailed() }
        return book
    }

    @Test func aBookIsFoundByIDUntilItIsDeleted() async throws {
        let library = try openLibrary()
        let book = try await importBook(into: library)

        #expect(library.book(id: book.id) == book)
        #expect(library.book(id: "unknown") == nil)

        await library.delete(book)

        #expect(library.book(id: book.id) == nil)
    }

    // MARK: Open Book

    @Test func openingABookFillsTheWindowUntilItIsClosed() async throws {
        let library = try openLibrary()
        let book = try await importBook(into: library)
        #expect(library.open == nil)

        library.openBook(id: book.id)
        #expect(library.open == book)

        library.closeBook()
        #expect(library.open == nil)
    }

    @Test func openingAnUnknownBookDoesNothing() throws {
        let library = try openLibrary()

        library.openBook(id: "unknown")

        #expect(library.open == nil)
    }

    @Test func aBookDeletedWhileOpenClosesItself() async throws {
        let library = try openLibrary()
        let book = try await importBook(into: library)
        library.openBook(id: book.id)

        await library.delete(book)

        #expect(library.open == nil)
    }

    @Test func deletingABookRemovesItsFile() async throws {
        let library = try openLibrary()
        let book = try await importBook(into: library)

        await library.delete(book)

        #expect(!FileManager.default.fileExists(atPath: book.file.path))
    }

    // MARK: Recently Read sort

    /// `libraryForSorting()` with "apple" read at 100 s, "Zebra" at 200 s and
    /// "Resume 2" at 300 s; "résumé 10" is never opened. Date Imported order
    /// (newest first) is Zebra, résumé 10, Resume 2, apple.
    func libraryForRecentlyRead() async throws -> LibraryModel {
        let library = try await libraryForSorting()
        for (title, seconds) in [("apple", 100.0), ("Zebra", 200), ("Resume 2", 300)] {
            let book = try #require(library.books.first { $0.title == title })
            await library.saveProgress(
                .pdf(pageIndex: 0, x: 0, y: 0), of: book, at: Date(timeIntervalSince1970: seconds))
        }
        return library
    }

    @Test func recentlyReadListsTheLastReadFirstAndNeverOpenedBooksLast() async throws {
        let library = try await libraryForRecentlyRead()

        library.sort.key = .recentlyRead

        try await expectListed(["Resume 2", "Zebra", "apple", "résumé 10"], in: library)
    }

    @Test func reversingRecentlyReadFlipsOnlyTheOpenedBooks() async throws {
        let library = try await libraryForRecentlyRead()

        library.sort = LibrarySort(key: .recentlyRead, isReversed: true)

        try await expectListed(["apple", "Zebra", "Resume 2", "résumé 10"], in: library)
    }

    @Test func openingABookMovesItToTheTopOfRecentlyRead() async throws {
        let library = try await libraryForRecentlyRead()
        library.sort.key = .recentlyRead
        try await expectListed(["Resume 2", "Zebra", "apple", "résumé 10"], in: library)
        let never = try #require(library.books.first { $0.title == "résumé 10" })

        await library.saveProgress(.pdf(pageIndex: 0, x: 0, y: 0), of: never, at: Date(timeIntervalSince1970: 400))

        #expect(library.visibleBooks(matching: "").map(\.title) == ["résumé 10", "Resume 2", "Zebra", "apple"])
    }

    @Test func theRecentlyReadChoiceIsRememberedOnThisDevice() throws {
        let library = try openLibrary()

        library.sort = LibrarySort(key: .recentlyRead, isReversed: true)

        #expect(try openLibrary().sort == LibrarySort(key: .recentlyRead, isReversed: true))
    }

    // MARK: Sort choice

    @Test func aSortChoiceNoLongerOfferedFallsBackToTheStandardOne() {
        let defaults = UserDefaults.isolated()
        defaults.set("lastOpened", forKey: "books.sort.key")

        #expect(LibrarySort(from: defaults) == .standard)
    }

    // MARK: Reading Progress

    @Test func voiceOverReadsAnOpenedBookWithItsPercentage() async throws {
        let core = try Library.open(root: root.path)
        let library = LibraryModel(core: core, coversFolder: coversFolder, defaults: defaults)
        let book = try await importBook(pageCount: 4, into: library)
        #expect(book.accessibilityLabel == book.title)

        _ = try core.saveProgress(
            id: book.id, progress: ReadingProgress(place: .pdf(pageIndex: 1, x: 0, y: 0), lastRead: 1_700_000_000_000),
            sort: LibrarySort.standard.core)
        let opened = try #require(try core.book(id: book.id))

        #expect(opened.percentRead == 50)
        #expect(opened.accessibilityLabel == "\(book.title), 50 % read")
    }

    // MARK: Page count

    /// Imports a PDF of `pageCount` pages.
    private func importBook(pageCount: Int, into library: LibraryModel) async throws -> Book {
        guard case .imported(let book) = await library.import(try writePDF(named: "a.pdf", pageCount: pageCount))
        else { throw ImportFailed() }
        return book
    }

    @Test func importSavesThePageCount() async throws {
        let library = try openLibrary()

        let book = try await importBook(pageCount: 3, into: library)

        #expect(book.pageCount == 3)
        #expect(try openLibrary().books.first?.pageCount == 3)
    }

    @Test func theBackgroundPassReadsAMissingPageCount() async throws {
        try importWithoutDetails(try writePDF(named: "a.pdf", pageCount: 3))

        let library = try openLibrary()
        #expect(library.books.first?.pageCount == nil)

        try await waitUntil { library.books.first?.pageCount == 3 }
        #expect(try openLibrary().books.first?.pageCount == 3)
    }

    /// Waits for the core's listing, which the model asks for off the main
    /// thread after a Sort change, to show `titles` for Library Search `search`.
    private func expectListed(
        _ titles: [String], matching search: String = "", in library: LibraryModel
    ) async throws {
        try await waitUntil { library.visibleBooks(matching: search).map(\.title) == titles }
    }

    /// Waits up to 10 s for `condition`, which the background pass makes true.
    private func waitUntil(_ condition: () -> Bool) async throws {
        for _ in 0..<1000 where !condition() { try await Task.sleep(for: .milliseconds(10)) }
        #expect(condition())
    }
}

struct ImportFailed: Error {}
