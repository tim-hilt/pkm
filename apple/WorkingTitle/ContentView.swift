import SwiftUI

/// The single window: the Gallery, or the open Drawing in the Canvas.
struct ContentView: View {
    @State private var gallery = Gallery(store: .appDrawings)

    var body: some View {
        Group {
            if let drawing = gallery.open {
                DrawingView(drawing: drawing, onBack: gallery.close)
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
        if ProcessInfo.processInfo.arguments.contains("-UITestingTemporaryStore") {
            let directory = FileManager.default.temporaryDirectory
                .appending(path: "WorkingTitleUITests-\(UUID().uuidString)", directoryHint: .isDirectory)
            try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
            return DrawingStore(directory: directory)
        }
        return DrawingStore(directory: .documentsDirectory)
    }
}
