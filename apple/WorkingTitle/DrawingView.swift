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
        .overlay { ActionBarView(controller: controller).dynamicTypeSize(...Self.largestChromeType) }
        .focusedSceneValue(controller)
        .overlay {
            GeometryReader { container in
                chrome(in: container.size)
            }
            .dynamicTypeSize(...Self.largestChromeType)
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

    /// Chrome text and glyphs follow Dynamic Type up to this size; beyond it
    /// they would outgrow Excalidraw's fixed 36 pt controls. The Canvas keeps
    /// the Drawing's own font sizes whatever the setting.
    static let largestChromeType = DynamicTypeSize.accessibility1

    /// Excalidraw's floating controls, laid out per `DrawingLayout` for the
    /// container's current size, so that they switch on every resize (Split
    /// View, Stage Manager, window resizing).
    @ViewBuilder private func chrome(in size: CGSize) -> some View {
        switch DrawingLayout(containerSize: size) {
        case .desktop: desktopChrome(in: size)
        case .mobile: mobileChrome(in: size)
        }
    }

    private func desktopChrome(in size: CGSize) -> some View {
        ZStack {
            HStack(spacing: 8) {
                BackControl(action: onBack)
                MainMenu(drawing: drawing, export: export, controller: controller)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)

            ToolBar(controller: controller)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)

            if showsProperties {
                PropertiesPanel(
                    controller: controller,
                    maxHeight: DrawingLayout.propertiesMaxHeight(containerHeight: size.height)
                )
                .padding(.top, DrawingLayout.propertiesTop - DrawingLayout.margin)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            }

            HStack(spacing: 9.6) {
                ZoomControls(controller: controller)
                HistoryControls(controller: controller)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottomLeading)

            #if !os(macOS)
                PasteImageControl(controller: controller)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottomTrailing)
            #endif
        }
        .padding(DrawingLayout.margin)
    }

    /// excalidraw.com's phone layout (docs/layout-reference §4): menu top
    /// left, everything else in the bottom bar within thumb reach; no zoom
    /// buttons (pinch), properties in a sheet.
    private func mobileChrome(in size: CGSize) -> some View {
        ZStack {
            HStack(spacing: 8) {
                BackControl(action: onBack, size: 32)
                MainMenu(drawing: drawing, export: export, controller: controller, size: 32)
            }
            .padding(12)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)

            VStack(spacing: 4) {
                MobileActionRow(controller: controller, showsProperties: showsProperties)
                MobileToolBar(controller: controller)
            }
            .frame(width: DrawingLayout.bottomBarWidth(containerWidth: size.width))
            .padding(.bottom, 10)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottom)
        }
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
    fileprivate func controlButton(width: CGFloat = 36, height: CGFloat = 36) -> some View {
        frame(width: width, height: height)
            .contentShape(Rectangle().inset(by: -(44 - min(width, height)) / 2))
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
    /// 36 pt on desktop, 32 pt on phones.
    var size: CGFloat = 36

    var body: some View {
        Button("Gallery", systemImage: "chevron.backward", action: action)
            .labelStyle(.iconOnly)
            .buttonStyle(.borderless)
            .controlButton(width: size, height: size)
            .background(controlFill, in: RoundedRectangle(cornerRadius: 8))
            .accessibilityIdentifier("back-to-gallery")
    }
}

/// Excalidraw's main menu (hamburger): exports the Drawing.
private struct MainMenu: View {
    let drawing: OpenDrawing
    let export: ExportedDrawing
    let controller: CanvasController
    /// 36 pt on desktop, 32 pt on phones.
    var size: CGFloat = 36
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
                    .disabled(!controller.hasSelection)
            }
        } label: {
            Label("Menu", systemImage: "line.3.horizontal")
        }
        .labelStyle(.iconOnly)
        .menuIndicator(.hidden)
        .buttonStyle(.borderless)
        .controlButton(width: size, height: size)
        .background(controlFill, in: RoundedRectangle(cornerRadius: 8))
        .accessibilityIdentifier("main-menu")
    }

    private func share(_ title: LocalizedStringKey, scope: ExportScope) -> some View {
        let png = ExportedPNG(name: drawing.name, editor: drawing.editor, scope: scope, background: background)
        return ShareLink(item: png, preview: SharePreview(drawing.name)) { Label(title, systemImage: "photo") }
    }
}

#if !os(macOS)
    /// Whether the chrome offers the Pencil-only drawing setting: on an iPad
    /// only. An iPhone has no Pencil, so the mode would leave it nothing that
    /// draws, and the core turns it on by itself only for a Pencil.
    @MainActor private var offersPencilOnlyDrawing: Bool {
        UIDevice.current.userInterfaceIdiom == .pad
    }
#endif

/// Excalidraw's toolbar island, in its order: Tool lock, then the Tools.
struct ToolBar: View {
    let controller: CanvasController

    static func name(_ tool: Tool) -> LocalizedStringResource { tools.first { $0.0 == tool }!.1 }
    static func symbol(_ tool: Tool) -> String { tools.first { $0.0 == tool }!.2 }

    private static let tools: [(Tool, LocalizedStringResource, String)] = [
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
                if offersPencilOnlyDrawing {
                    divider
                    toolButton(
                        "Pencil-only drawing", symbol: "applepencil", checked: controller.pencilOnlyDrawing
                    ) { controller.setPencilOnlyDrawing(!controller.pencilOnlyDrawing) }
                }
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
        _ name: LocalizedStringResource, symbol: String, checked: Bool, action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.subheadline)
                .foregroundStyle(checked ? checkedGlyph : .primary)
                .controlButton()
                .background(checked ? checkedFill : .clear, in: RoundedRectangle(cornerRadius: 8))
        }
        .buttonStyle(.plain)
        .accessibilityLabel(Text(name))
        .accessibilityAddTraits(checked ? .isSelected : [])
    }
}

/// The phone toolbar island (docs/layout-reference §4.2): one slot per Tool
/// family, spread over the bar's width; tapping a family's armed Tool again
/// offers the rest of the family. Tool lock and Pencil-only drawing, which
/// have no slot on excalidraw.com's phone bar, sit in the ⋮ menu (Pencil-only
/// drawing only on an iPad, see `offersPencilOnlyDrawing`).
private struct MobileToolBar: View {
    let controller: CanvasController
    /// Per slot, the Tool of its family last chosen, so the slot keeps showing it.
    @State private var lastChosen: [Int: Tool] = [:]
    @State private var familyShown: Int?

    var body: some View {
        HStack(spacing: 4) {
            ForEach(MobileToolSlot.all.indices, id: \.self) { index in
                slot(index)
                Spacer(minLength: 0)
            }
            moreMenu
        }
        .padding(4)
        .island()
    }

    private func slot(_ index: Int) -> some View {
        let slot = MobileToolSlot.all[index]
        let shown = slot.shownTool(current: controller.tool, last: lastChosen[index])
        let armed = controller.tool == shown
        return MobileToolButton(tool: shown, checked: armed) {
            if armed && slot.tools.count > 1 {
                familyShown = index
            } else {
                controller.setTool(shown)
            }
        }
        .popover(
            isPresented: Binding(
                get: { familyShown == index }, set: { if !$0 { familyShown = nil } })
        ) {
            HStack(spacing: 4) {
                ForEach(slot.tools, id: \.self) { tool in
                    MobileToolButton(tool: tool, checked: controller.tool == tool) {
                        lastChosen[index] = tool
                        controller.setTool(tool)
                        familyShown = nil
                    }
                }
            }
            .padding(8)
            .presentationCompactAdaptation(.popover)
        }
    }

    private var moreMenu: some View {
        Menu {
            Toggle(
                "Keep tool selected",
                isOn: Binding(
                    get: { controller.toolLock }, set: { controller.setToolLock($0) }))
            #if !os(macOS)
                if offersPencilOnlyDrawing {
                    Toggle(
                        "Pencil-only drawing",
                        isOn: Binding(
                            get: { controller.pencilOnlyDrawing }, set: { controller.setPencilOnlyDrawing($0) }))
                }
            #endif
        } label: {
            Image(systemName: "ellipsis").rotationEffect(.degrees(90)).controlButton()
        }
        .menuIndicator(.hidden)
        .buttonStyle(.plain)
        .accessibilityLabel("More tools")
    }
}

/// A Tool's 36 pt button in the phone toolbar, radius-4 highlight when armed.
private struct MobileToolButton: View {
    let tool: Tool
    let checked: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: ToolBar.symbol(tool))
                .font(.subheadline)
                .foregroundStyle(checked ? checkedGlyph : .primary)
                .controlButton()
                .background(checked ? checkedFill : .clear, in: RoundedRectangle(cornerRadius: 4))
        }
        .buttonStyle(.plain)
        .accessibilityLabel(Text(ToolBar.name(tool)))
        .accessibilityAddTraits(checked ? .isSelected : [])
    }
}

/// The phone action row over the toolbar (docs/layout-reference §4.3):
/// properties on the left while a Tool or Selection has them, opening in a
/// sheet (the spec's native adaptation of excalidraw.com's popovers); paste
/// and undo/redo on the right.
private struct MobileActionRow: View {
    let controller: CanvasController
    let showsProperties: Bool
    @State private var sheetShown = false

    var body: some View {
        HStack(spacing: 6) {
            if showsProperties {
                Button("Properties", systemImage: "slider.horizontal.3") { sheetShown = true }
                    .controlButton(width: 32, height: 32)
                    .background(.background, in: RoundedRectangle(cornerRadius: 8))
                    .accessibilityIdentifier("properties-sheet")
            }
            Spacer(minLength: 0)
            #if !os(macOS)
                PasteImageControl(controller: controller)
            #endif
            Button("Undo", systemImage: "arrow.uturn.backward") { controller.undo() }
                .controlButton(width: 32, height: 32)
                .disabled(!controller.canUndo)
            Button("Redo", systemImage: "arrow.uturn.forward") { controller.redo() }
                .controlButton(width: 32, height: 32)
                .disabled(!controller.canRedo)
        }
        .labelStyle(.iconOnly)
        .buttonStyle(.borderless)
        .frame(height: 43.2)
        .sheet(
            isPresented: Binding(
                get: { sheetShown && showsProperties }, set: { sheetShown = $0 })
        ) {
            PropertiesPanel(controller: controller, width: nil)
                .padding()
                .presentationDetents([.medium, .large])
                .presentationBackgroundInteraction(.enabled(upThrough: .medium))
        }
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
