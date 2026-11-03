import SwiftUI
import UniformTypeIdentifiers

struct ContentView: View {
    @State private var editor: Editor
    /// Requested once: panning and zooming move the viewport, never the
    /// geometry, so nothing here depends on the viewport and this view is not
    /// re-evaluated while navigating.
    @State private var geometry: Geometry
    @State private var controller = CanvasController()

    init() {
        // An unreadable sample leaves an empty Canvas rather than a crash.
        let editor = (try? SampleDrawing.makeEditor()) ?? Editor.withExcalifont()
        _editor = State(initialValue: editor)
        _geometry = State(initialValue: editor.geometry())
    }

    var body: some View {
        CanvasView(editor: editor, geometry: geometry, controller: controller)
            .ignoresSafeArea()
            .overlay { ActionBarView(controller: controller) }
            .overlay(alignment: .top) { ToolBar(controller: controller) }
            .overlay(alignment: .bottom) { ZoomControls(controller: controller) }
            .overlay(alignment: .bottomLeading) { HistoryControls(controller: controller) }
            .overlay(alignment: .topLeading) { PropertiesPanel(controller: controller) }
            #if !os(macOS)
                .overlay(alignment: .bottomTrailing) { PasteImageControl(controller: controller) }
            #endif
    }
}

/// Minimal Tool controls; their final placement comes with the layout.
private struct ToolBar: View {
    let controller: CanvasController

    private static let tools: [(Tool, String, String)] = [
        (.selection, "Selection", "cursorarrow"),
        (.hand, "Hand", "hand.raised"),
        (.rectangle, "Rectangle", "rectangle"),
        (.diamond, "Diamond", "diamond"),
        (.ellipse, "Ellipse", "circle"),
        (.line, "Line", "line.diagonal"),
        (.arrow, "Arrow", "arrow.up.right"),
        (.freedraw, "Draw", "pencil.tip"),
        (.text, "Text", "textformat"),
        (.eraser, "Eraser", "eraser"),
    ]

    var body: some View {
        HStack(spacing: 12) {
            ForEach(Self.tools, id: \.0) { tool, name, symbol in
                Toggle(
                    isOn: Binding(
                        get: { controller.tool == tool },
                        set: { _ in controller.setTool(tool) })
                ) { Label(name, systemImage: symbol) }
                .toggleStyle(.button)
            }
            Divider().frame(height: 20)
            Toggle(
                isOn: Binding(
                    get: { controller.toolLock },
                    set: { controller.setToolLock($0) })
            ) { Label("Keep tool selected", systemImage: controller.toolLock ? "lock" : "lock.open") }
            .toggleStyle(.button)
        }
        .labelStyle(.iconOnly)
        .padding(10)
        .background(.regularMaterial, in: Capsule())
        .padding()
    }
}

/// Minimal navigation controls; their final placement comes with the layout.
private struct ZoomControls: View {
    let controller: CanvasController

    var body: some View {
        HStack(spacing: 16) {
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

/// Minimal undo and redo buttons; their final placement comes with the layout.
private struct HistoryControls: View {
    let controller: CanvasController

    var body: some View {
        HStack(spacing: 16) {
            Button("Undo", systemImage: "arrow.uturn.backward") { controller.undo() }
                .disabled(!controller.canUndo)
            Button("Redo", systemImage: "arrow.uturn.forward") { controller.redo() }
                .disabled(!controller.canRedo)
        }
        .labelStyle(.iconOnly)
        .padding(10)
        .background(.regularMaterial, in: Capsule())
        .padding()
    }
}

#if !os(macOS)
    /// The system's paste button: the one way to paste an image on an iPhone,
    /// which has no keyboard shortcut or menu to do it. Pasting through it
    /// needs no permission prompt.
    private struct PasteImageControl: UIViewRepresentable {
        let controller: CanvasController

        /// What the button pastes into. The Canvas is not there when the button
        /// is made, so this hands the paste on to it when it happens.
        @MainActor final class Target: UIResponder {
            let controller: CanvasController

            init(_ controller: CanvasController) {
                self.controller = controller
                super.init()
                pasteConfiguration = UIPasteConfiguration(acceptableTypeIdentifiers: [UTType.image.identifier])
            }

            override func paste(itemProviders: [NSItemProvider]) {
                controller.canvas?.paste(itemProviders: itemProviders)
            }
        }

        func makeCoordinator() -> Target { Target(controller) }

        func makeUIView(context: Context) -> UIPasteControl {
            let configuration = UIPasteControl.Configuration()
            configuration.displayMode = .iconOnly
            configuration.baseBackgroundColor = .secondarySystemBackground
            configuration.baseForegroundColor = .label
            configuration.cornerStyle = .capsule
            let control = UIPasteControl(configuration: configuration)
            control.target = context.coordinator
            return control
        }

        func updateUIView(_ control: UIPasteControl, context: Context) {}
    }
#endif
