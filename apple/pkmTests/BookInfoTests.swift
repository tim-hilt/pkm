import Foundation
import Testing

@testable import pkm

@MainActor @Suite struct BookInfoTests {
    private func openLibrary() throws -> LibraryModel {
        let root = FileManager.default.temporaryDirectory.appending(
            path: UUID().uuidString, directoryHint: .isDirectory)
        return LibraryModel(
            core: try Library.open(root: root.path), coversFolder: root.appending(path: "covers"),
            defaults: .isolated())
    }

    @Test func tellsTitleAuthorFormatSizeAndDateImportedInThatOrder() async throws {
        let library = try openLibrary()
        let source = try writePDF(named: "a.pdf", metadataTitle: "Moby-Dick", author: "Melville", paddedTo: 2_000)
        guard case .imported(let book) = await library.import(source) else { throw ImportFailed() }

        let rows = BookInfo(book).rows

        #expect(rows.map { String(localized: $0.label) } == ["Title", "Author", "Format", "File Size", "Imported"])
        #expect(rows.map(\.value).prefix(3) == ["Moby-Dick", "Melville", "PDF"])
        #expect(rows[3].value == Int64(2_000).formatted(.byteCount(style: .file)))
    }

    @Test func leavesOutAnAuthorThePDFLacks() async throws {
        let library = try openLibrary()
        guard case .imported(let book) = await library.import(try writePDF(named: "a.pdf")) else {
            throw ImportFailed()
        }

        #expect(
            BookInfo(book).rows.map { String(localized: $0.label) } == ["Title", "Format", "File Size", "Imported"])
    }
}
