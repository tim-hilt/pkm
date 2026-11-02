import Foundation

@testable import WorkingTitle

/// Scenes for renderer tests, made the way the app makes its own: by loading
/// a Drawing into the core's `Editor`.
enum TestScenes {
    /// A Drawing with one rectangle Element at (50, 100), 280 x 180.
    static func rectangle() -> Geometry {
        geometry(
            of: """
                {"type":"excalidraw","version":2,"elements":[
                  {"id":"box","type":"rectangle","x":50,"y":100,"width":280,"height":180}]}
                """)
    }

    static func geometry(of drawing: String) -> Geometry {
        let editor = Editor()
        try! editor.load(bytes: Data(drawing.utf8))
        return editor.geometry()
    }
}
