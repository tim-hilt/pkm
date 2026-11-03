import SwiftUI
import UniformTypeIdentifiers

/// A Drawing in the Canvas with its controls, kept saved while it is open.
struct DrawingView: View {
    let drawing: OpenDrawing
    /// The Drawing as a `.excalidraw` file for the share sheet.
    let export: ExportedDrawing
    let onBack: () -> Void
    /// Requested once: panning and zooming move the viewport, never the
    /// geometry, so nothing here depends on the viewport and this view is not
    /// re-evaluated while navigating.
    @State private var geometry: Geometry
    @State private var controller = CanvasController()
    @Environment(\.scenePhase) private var scenePhase

    init(drawing: OpenDrawing, export: ExportedDrawing, onBack: @escaping () -> Void) {
        self.drawing = drawing
        self.export = export
        self.onBack = onBack
        _geometry = State(initialValue: drawing.editor.geometry())
    }

    var body: some View {
        CanvasView(
            editor: drawing.editor, geometry: geometry, controller: controller,
            onSceneChange: { drawing.editorDidChange() }
        )
        .ignoresSafeArea()
        .overlay { ActionBarView(controller: controller) }
        .overlay(alignment: .top) { ToolBar(controller: controller) }
        .overlay(alignment: .bottom) {
            VStack(spacing: 0) {
                HStack {
                    BackControl(action: onBack)
                    ShareLink(item: export, preview: SharePreview(drawing.name)) {
                        Label("Export", systemImage: "square.and.arrow.up")
                    }
                    .accessibilityIdentifier("export-drawing")
                }
                ZoomControls(controller: controller)
            }
        }
        .overlay(alignment: .bottomLeading) { HistoryControls(controller: controller) }
        .overlay(alignment: .topLeading) { PropertiesPanel(controller: controller) }
        #if !os(macOS)
            .overlay(alignment: .bottomTrailing) { PasteImageControl(controller: controller) }
        #endif
        // Whatever leaves the Drawing (background, closing the window, quitting)
        // saves it at once.
        .onChange(of: scenePhase) { _, phase in
            if phase != .active { drawing.saveNow() }
        }
        .onDisappear { drawing.saveNow() }
        #if os(macOS)
            .onReceive(NotificationCenter.default.publisher(for: NSApplication.willTerminateNotification)) { _ in
                drawing.saveNow()
            }
        #endif
    }
}

/// Back to the Gallery; its final placement comes with the layout.
private struct BackControl: View {
    let action: () -> Void

    var body: some View {
        Button("Gallery", systemImage: "chevron.backward", action: action)
            .accessibilityIdentifier("back-to-gallery")
            .padding(10)
            .background(.regularMaterial, in: Capsule())
            .padding(.horizontal)
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
            #if !os(macOS)
                Toggle(
                    isOn: Binding(
                        get: { controller.pencilOnlyDrawing },
                        set: { controller.setPencilOnlyDrawing($0) })
                ) { Label("Pencil-only drawing", systemImage: "applepencil") }
                .toggleStyle(.button)
            #endif
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
    /// The system's paste button: the one way to paste an image or Elements on an iPhone,
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
                pasteConfiguration = UIPasteConfiguration(acceptableTypeIdentifiers: CanvasMetalView.pasteTypes)
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

        /// Its own size: left to fill the space it is offered, it covers the
        /// whole Canvas and takes every tap.
        func sizeThatFits(_ proposal: ProposedViewSize, uiView: UIPasteControl, context: Context) -> CGSize? {
            uiView.intrinsicContentSize
        }
    }
#endif
