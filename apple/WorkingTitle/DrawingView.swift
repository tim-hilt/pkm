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
        .overlay {
            GeometryReader { container in
                chrome(in: container.size)
            }
        }
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

    /// Excalidraw's floating controls, placed per `DrawingLayout` for the
    /// container's current size, so that they follow every resize. The
    /// mobile placement is refined by its own ticket.
    private func chrome(in size: CGSize) -> some View {
        let layout = DrawingLayout(containerSize: size)
        let margin = DrawingLayout.margin
        return ZStack {
            HStack(spacing: 8) {
                BackControl(action: onBack)
                MainMenu(drawing: drawing, export: export)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)

            ToolBar(controller: controller)
                .frame(
                    maxWidth: .infinity, maxHeight: .infinity,
                    alignment: layout == .desktop ? .top : .bottom)

            if showsProperties {
                PropertiesPanel(
                    controller: controller,
                    maxHeight: DrawingLayout.propertiesMaxHeight(containerHeight: size.height)
                )
                .padding(.top, DrawingLayout.propertiesTop - margin)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            }

            if layout == .desktop {
                HStack(spacing: 9.6) {
                    ZoomControls(controller: controller)
                    HistoryControls(controller: controller)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottomLeading)
            } else {
                HStack {
                    ZoomControls(controller: controller)
                    Spacer()
                    HistoryControls(controller: controller)
                }
                .padding(.bottom, 60)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottom)
            }

            #if !os(macOS)
                PasteImageControl(controller: controller)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottomTrailing)
            #endif
        }
        .padding(margin)
    }

    /// Excalidraw shows no properties for the Tools that make no Elements.
    private var showsProperties: Bool {
        controller.properties != nil && controller.tool != .hand && controller.tool != .eraser
    }
}

/// Excalidraw's island: white, radius 8, its soft shadow.
private struct Island: ViewModifier {
    func body(content: Content) -> some View {
        content
            .background(.background, in: RoundedRectangle(cornerRadius: 8))
            .shadow(color: .black.opacity(0.17), radius: 0.5)
            .shadow(color: .black.opacity(0.08), radius: 1.5)
            .shadow(color: .black.opacity(0.05), radius: 7, y: 7)
    }
}

extension View {
    fileprivate func island() -> some View { modifier(Island()) }

    /// Excalidraw's 36 pt control, its hit area grown to Apple's 44 pt.
    fileprivate func controlButton(width: CGFloat = 36) -> some View {
        frame(width: width, height: 36)
            .contentShape(Rectangle().inset(by: -4))
    }
}

/// Excalidraw's grey control group fill, `#ececf4`.
private let controlFill = Color(red: 0xEC / 255, green: 0xEC / 255, blue: 0xF4 / 255)
/// Excalidraw's checked-tool fill `#e0dfff` and glyph `#030064`.
private let checkedFill = Color(red: 0xE0 / 255, green: 0xDF / 255, blue: 1)
private let checkedGlyph = Color(red: 3 / 255, green: 0, blue: 0x64 / 255)

/// Back to the Gallery, in the corner Excalidraw keeps for its main menu.
private struct BackControl: View {
    let action: () -> Void

    var body: some View {
        Button("Gallery", systemImage: "chevron.backward", action: action)
            .labelStyle(.iconOnly)
            .buttonStyle(.borderless)
            .controlButton()
            .background(controlFill, in: RoundedRectangle(cornerRadius: 8))
            .accessibilityIdentifier("back-to-gallery")
    }
}

/// Excalidraw's main menu (hamburger): exports the Drawing.
private struct MainMenu: View {
    let drawing: OpenDrawing
    let export: ExportedDrawing
    @State private var background = true

    var body: some View {
        Menu {
            ShareLink(item: export, preview: SharePreview(drawing.name)) {
                Label("Export", systemImage: "square.and.arrow.up")
            }
            .accessibilityIdentifier("export-drawing")
            Section("Export PNG") {
                Toggle("Background", isOn: $background)
                share("Export Scene as PNG", scope: .scene)
                share("Export Selection as PNG", scope: .selection)
            }
        } label: {
            Label("Menu", systemImage: "line.3.horizontal")
        }
        .labelStyle(.iconOnly)
        .menuIndicator(.hidden)
        .buttonStyle(.borderless)
        .controlButton()
        .background(controlFill, in: RoundedRectangle(cornerRadius: 8))
        .accessibilityIdentifier("main-menu")
    }

    private func share(_ title: LocalizedStringKey, scope: ExportScope) -> some View {
        let png = ExportedPNG(name: drawing.name, editor: drawing.editor, scope: scope, background: background)
        return ShareLink(item: png, preview: SharePreview(drawing.name)) { Label(title, systemImage: "photo") }
    }
}

/// Excalidraw's toolbar island, in its order: Tool lock, then the Tools.
private struct ToolBar: View {
    let controller: CanvasController

    private static let tools: [(Tool, String, String)] = [
        (.hand, "Hand", "hand.raised"),
        (.selection, "Selection", "cursorarrow"),
        (.rectangle, "Rectangle", "square"),
        (.diamond, "Diamond", "diamond"),
        (.ellipse, "Ellipse", "circle"),
        (.arrow, "Arrow", "arrow.right"),
        (.line, "Line", "line.diagonal"),
        (.freedraw, "Draw", "pencil"),
        (.text, "Text", "textformat.alt"),
        (.eraser, "Eraser", "eraser"),
    ]

    var body: some View {
        HStack(spacing: 4) {
            toolButton(
                "Keep tool selected", symbol: controller.toolLock ? "lock" : "lock.open",
                checked: controller.toolLock
            ) { controller.setToolLock(!controller.toolLock) }
            divider
            ForEach(Self.tools, id: \.0) { tool, name, symbol in
                toolButton(name, symbol: symbol, checked: controller.tool == tool) {
                    controller.setTool(tool)
                }
            }
            #if !os(macOS)
                divider
                toolButton(
                    "Pencil-only drawing", symbol: "applepencil", checked: controller.pencilOnlyDrawing
                ) { controller.setPencilOnlyDrawing(!controller.pencilOnlyDrawing) }
            #endif
        }
        .padding(4)
        .island()
    }

    private var divider: some View {
        Rectangle().fill(Color(red: 0xF1 / 255, green: 0xF0 / 255, blue: 1))
            .frame(width: 1, height: 24).padding(.horizontal, 4)
    }

    private func toolButton(
        _ name: String, symbol: String, checked: Bool, action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: 15))
                .foregroundStyle(checked ? checkedGlyph : .primary)
                .controlButton()
                .background(checked ? checkedFill : .clear, in: RoundedRectangle(cornerRadius: 8))
        }
        .buttonStyle(.plain)
        .accessibilityLabel(name)
        .accessibilityAddTraits(checked ? .isSelected : [])
    }
}

/// Excalidraw's zoom group: out, reset, in; plus zoom to fit (ticket 09).
private struct ZoomControls: View {
    let controller: CanvasController

    var body: some View {
        HStack(spacing: 0) {
            Button("Zoom out", systemImage: "minus") { controller.zoomOut() }.controlButton()
            Button("Reset zoom", systemImage: "1.magnifyingglass") { controller.resetZoom() }
                .controlButton(width: 44)
            Button("Zoom in", systemImage: "plus") { controller.zoomIn() }.controlButton()
            Button("Zoom to fit", systemImage: "arrow.up.left.and.down.right.magnifyingglass") {
                controller.zoomToFit()
            }
            .controlButton()
        }
        .controlGroup()
    }
}

/// Excalidraw's undo/redo group, enabled as the history allows.
private struct HistoryControls: View {
    let controller: CanvasController

    var body: some View {
        HStack(spacing: 0) {
            Button("Undo", systemImage: "arrow.uturn.backward") { controller.undo() }
                .controlButton()
                .disabled(!controller.canUndo)
            Button("Redo", systemImage: "arrow.uturn.forward") { controller.redo() }
                .controlButton()
                .disabled(!controller.canRedo)
        }
        .controlGroup()
    }
}

extension View {
    /// Excalidraw's grey bottom-left button groups.
    fileprivate func controlGroup() -> some View {
        labelStyle(.iconOnly)
            .buttonStyle(.borderless)
            .background(controlFill, in: RoundedRectangle(cornerRadius: 8))
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
