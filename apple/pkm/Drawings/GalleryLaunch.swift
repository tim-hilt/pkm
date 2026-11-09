import Foundation

extension DrawingStore {
    /// The app's Documents folder, which Files and Finder show, with the
    /// caches in Caches. UI tests ask for empty temporary folders instead.
    static var appDrawings: DrawingStore {
        appDrawings(temporary: ProcessInfo.processInfo.arguments.contains("-UITestingTemporaryStore"))
    }

    static func appDrawings(temporary: Bool) -> DrawingStore {
        if temporary {
            let root = FileManager.default.temporaryDirectory
                .appending(path: "pkmUITests-\(UUID().uuidString)", directoryHint: .isDirectory)
            let directory = root.appending(path: "Documents", directoryHint: .isDirectory)
            try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
            return DrawingStore(directory: directory, cacheDirectory: root.appending(path: "Caches"))
        }
        return DrawingStore(
            directory: .documentsDirectory,
            cacheDirectory: .cachesDirectory.appending(path: "Drawings", directoryHint: .isDirectory))
    }
}

extension Gallery {
    /// The Gallery for this launch. `-UITestingOpenSampleDrawing` (used by
    /// `make screenshot`) starts in an empty temporary store with the sample
    /// Drawing already open, so agents can see the Canvas without tapping.
    @MainActor
    static func forLaunch() -> Gallery {
        let arguments = ProcessInfo.processInfo.arguments
        if AppNavigation.opensSettingsAtLaunch { return Gallery(store: .appDrawings(temporary: true)) }
        guard arguments.contains("-UITestingOpenSampleDrawing") else { return Gallery(store: .appDrawings) }
        let store = DrawingStore.appDrawings(temporary: true)
        let name = try? store.add(Data(contentsOf: SampleDrawing.url), named: "sample")
        let gallery = Gallery(store: store)
        if let sample = gallery.drawings.first(where: { $0.name == name }) { gallery.openDrawing(itemID: sample.id) }
        return gallery
    }
}
