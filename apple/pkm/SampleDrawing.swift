import Foundation

/// The Drawing bundled with the app, shown until the Gallery can open others.
enum SampleDrawing {
    private final class BundleToken {}

    static var url: URL {
        Bundle(for: BundleToken.self).url(forResource: "sample", withExtension: "excalidraw")!
    }

    /// An `Editor` with the sample loaded.
    static func makeEditor() throws -> Editor {
        let editor = Editor.withExcalifont()
        try editor.load(bytes: Data(contentsOf: url))
        return editor
    }
}
