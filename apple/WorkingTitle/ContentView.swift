import SwiftUI

struct ContentView: View {
    @State private var editor: Editor
    /// Requested once: panning and zooming move the viewport, never the
    /// geometry, so nothing here depends on the viewport and this view is not
    /// re-evaluated while navigating.
    @State private var geometry: Geometry
    @State private var controller = CanvasController()
    @State private var handTool = false

    init() {
        // An unreadable sample leaves an empty Canvas rather than a crash.
        let editor = (try? SampleDrawing.makeEditor()) ?? Editor()
        _editor = State(initialValue: editor)
        _geometry = State(initialValue: editor.geometry())
    }

    var body: some View {
        CanvasView(editor: editor, geometry: geometry, controller: controller)
            .ignoresSafeArea()
            .overlay(alignment: .bottom) { ZoomControls(controller: controller, handTool: $handTool) }
    }
}

/// Minimal navigation controls; their final placement comes with the layout.
private struct ZoomControls: View {
    let controller: CanvasController
    @Binding var handTool: Bool

    var body: some View {
        HStack(spacing: 16) {
            Toggle(isOn: $handTool) { Label("Hand", systemImage: "hand.raised") }
                .toggleStyle(.button)
                .onChange(of: handTool) { _, on in controller.setTool(on ? .hand : .selection) }
            Button("Zoom out", systemImage: "minus") { controller.zoomOut() }
            Button("Reset zoom", systemImage: "1.magnifyingglass") { controller.resetZoom() }
            Button("Zoom in", systemImage: "plus") { controller.zoomIn() }
            Button("Zoom to fit", systemImage: "arrow.up.left.and.down.right.magnifyingglass") {
                controller.zoomToFit()
            }
        }
        .labelStyle(.iconOnly)
        .padding(10)
        .background(.regularMaterial, in: Capsule())
        .padding()
    }
}
