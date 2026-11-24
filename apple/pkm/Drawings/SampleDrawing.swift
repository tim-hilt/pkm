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

    /// For `make screenshot SCREEN=dimensions` (`-UITestingHoldResize`): replaces
    /// the Scene with one turned rectangle and holds a mouse mid-resize on its
    /// south-east handle, so that its Dimensions are on show.
    static func holdResize(_ editor: Editor) throws {
        let rectangle = """
            {"type":"excalidraw","version":2,"elements":[{"id":"a","type":"rectangle","x":280,"y":300,
            "width":60,"height":50,"angle":0.3,"strokeWidth":2,"backgroundColor":"#ffc9c9",
            "roundness":null,"version":3,"versionNonce":7}]}
            """
        try editor.load(bytes: Data(rectangle.utf8))
        let centre = (x: 310.0, y: 325.0)
        func event(_ phase: PointerPhase, _ x: Double, _ y: Double, at time: Double) -> PointerEvent {
            PointerEvent(
                id: 1, phase: phase, x: x, y: y, kind: .mouse, button: .primary, pressure: 0.5, timestamp: time,
                modifiers: Modifiers(shift: false, ctrl: false, alt: false, meta: false))
        }
        editor.pointer(event: event(.down, centre.x, centre.y, at: 0))
        editor.pointer(event: event(.up, centre.x, centre.y, at: 0.05))
        // The handle is 6 beyond the corner, turned with the box.
        let (s, c) = (sin(0.3), cos(0.3))
        let handle = (x: centre.x + 36 * c - 31 * s, y: centre.y + 36 * s + 31 * c)
        editor.pointer(event: event(.down, handle.x, handle.y, at: 1))
        editor.pointer(event: event(.move, handle.x + 10, handle.y + 5, at: 1.1))
        editor.pointer(event: event(.move, handle.x + 20, handle.y + 10, at: 1.2))
    }
}
