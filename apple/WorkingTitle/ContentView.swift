import SwiftUI

struct ContentView: View {
    @State private var editor: Editor
    /// Requested once: panning and zooming move the viewport, never the
    /// geometry, so nothing here depends on the viewport and this view is not
    /// re-evaluated while navigating.
    @State private var geometry: Geometry

    init() {
        // An unreadable sample leaves an empty Canvas rather than a crash.
        let editor = (try? SampleDrawing.makeEditor()) ?? Editor()
        _editor = State(initialValue: editor)
        _geometry = State(initialValue: editor.geometry())
    }

    var body: some View {
        CanvasView(editor: editor, geometry: geometry)
            .ignoresSafeArea()
    }
}
