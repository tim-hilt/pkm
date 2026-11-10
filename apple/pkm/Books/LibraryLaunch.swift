import Foundation

extension LibraryModel {
    /// The Books folder in the app's Documents, which Files and Finder show.
    static let documentsFolder = URL.documentsDirectory.appending(path: "Books", directoryHint: .isDirectory)

    /// The Library for this launch: the Books folder, with Covers in Caches and
    /// the sort choice in the standard defaults. For UI tests and screenshots
    /// (`-UITesting…`) it is empty and temporary, with sample Books when
    /// launched with `-UITestingOpenBooks`.
    @MainActor
    static func forLaunch() -> LibraryModel {
        let arguments = ProcessInfo.processInfo.arguments
        guard arguments.contains(where: { $0.hasPrefix("-UITesting") }) else {
            return LibraryModel(
                core: openOrTemporary(documentsFolder), coversFolder: .cachesDirectory.appending(path: "Covers"),
                defaults: .standard)
        }
        let root = temporaryRoot()
        let core = openOrTemporary(root.appending(path: "Books"))
        if arguments.contains("-UITestingOpenBooks") { SampleBooks.import(into: core, sort: LibrarySort.standard.core) }
        return LibraryModel(
            core: core, coversFolder: root.appending(path: "Covers"),
            defaults: UserDefaults(suiteName: root.lastPathComponent)!)
    }

    private static func temporaryRoot() -> URL {
        FileManager.default.temporaryDirectory.appending(
            path: "pkmUITests-\(UUID().uuidString)", directoryHint: .isDirectory)
    }

    /// The Library in `folder`, created if needed. Where that fails, a temporary one:
    /// the app still runs, its Books just don't stay.
    private static func openOrTemporary(_ folder: URL) -> Library {
        (try? Library.open(root: folder.path))
            ?? (try! Library.open(root: temporaryRoot().appending(path: "Books").path))
    }
}
