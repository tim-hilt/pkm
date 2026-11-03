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
        // Share sheet / open-in: the file becomes a new Drawing and is shown.
        .onOpenURL { url in
            gallery.close()
            if let name = gallery.importDrawings(from: [url]).first { gallery.openDrawing(named: name) }
        }
    }
}

extension DrawingStore {
    /// The app's Documents folder, which Files and Finder show. UI tests ask
    /// for an empty temporary folder instead.
    static var appDrawings: DrawingStore {
        appDrawings(temporary: ProcessInfo.processInfo.arguments.contains("-UITestingTemporaryStore"))
    }

    static func appDrawings(temporary: Bool) -> DrawingStore {
        if temporary {
            let directory = FileManager.default.temporaryDirectory
                .appending(path: "WorkingTitleUITests-\(UUID().uuidString)", directoryHint: .isDirectory)
            try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
            return DrawingStore(directory: directory)
        }
        return DrawingStore(directory: .documentsDirectory)
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
