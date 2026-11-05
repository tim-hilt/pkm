import SwiftUI

/// The single window: the Gallery, or the open Drawing in the Canvas.
struct ContentView: View {
    @State private var gallery = Gallery.forLaunch()

    var body: some View {
        Group {
            if let drawing = gallery.open {
                DrawingView(
                    drawing: drawing, export: ExportedDrawing(name: drawing.name, gallery: gallery),
                    onBack: gallery.close
                )
                // A fresh view per Drawing: its geometry is requested once.
                .id(drawing.name)
            } else {
                GalleryView(gallery: gallery)
            }
        }
        .onOpenURL { url in gallery.openFile(at: url) }
        // Dark mode is out of scope for v1 (spec): the Canvas is always
        // light, and the chrome uses Excalidraw's light-theme colours, which
        // would clash with the system's dark materials.
        .preferredColorScheme(.light)
    }
}

extension DrawingStore {
    /// The app's Documents folder, which Files and Finder show, with the
    /// caches in Caches. UI tests ask for empty temporary folders instead.
    static var appDrawings: DrawingStore {
        appDrawings(temporary: ProcessInfo.processInfo.arguments.contains("-UITestingTemporaryStore"))
    }

    static func appDrawings(temporary: Bool) -> DrawingStore {
        if temporary {
            let root = FileManager.default.temporaryDirectory
                .appending(path: "WorkingTitleUITests-\(UUID().uuidString)", directoryHint: .isDirectory)
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
        guard arguments.contains("-UITestingOpenSampleDrawing") else { return Gallery(store: .appDrawings) }
        let gallery = Gallery(store: .appDrawings(temporary: true))
        if let name = gallery.importDrawings(from: [SampleDrawing.url]).first { gallery.openDrawing(named: name) }
        return gallery
    }
}
