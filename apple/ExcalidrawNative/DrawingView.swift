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
    /// Decides whether the toolbar is in the navigation bar (desktop) or at
    /// the bottom (mobile); follows the Canvas container's size.
    @State private var layout = DrawingLayout.desktop
    #if !os(macOS)
        @State private var lastChosen: [Int: Tool] = [:]
        @State private var familyShown: Int?
    #endif
    @Environment(\.scenePhase) private var scenePhase

    init(drawing: OpenDrawing, export: ExportedDrawing, onBack: @escaping () -> Void) {
        self.drawing = drawing
        self.export = export
        self.onBack = onBack
        _geometry = State(initialValue: drawing.editor.geometry())
    }

    var body: some View {
        // The system's navigation bar (the window's toolbar on a Mac) carries
        // the Gallery button, the desktop layout's toolbar and, on iPhone and
        // iPad, the main menu; on a Mac the menu's items are in the menu bar
        // (`CanvasCommands`). The Drawing's name is not shown.
        NavigationStack {
            canvas
                #if os(macOS)
                    .navigationTitle(drawing.name)
                    .toolbar(removing: .title)
                    .hidesTitlebarSeparator()
                #endif
                .toolbar {
                    ToolbarItem(placement: .navigation) {
                        Button("Gallery", systemImage: "chevron.backward", action: onBack)
                            .accessibilityIdentifier("back-to-gallery")
                    }
                    #if os(macOS)
                        DesktopToolbar(controller: controller)
                    #else
                        if layout == .desktop {
                            DesktopToolbar(controller: controller)
                        } else {
                            MobileToolbar(controller: controller, lastChosen: $lastChosen, familyShown: $familyShown)
                        }
                    #endif
                    #if !os(macOS)
                        ToolbarItem(placement: .primaryAction) {
                            MainMenu(
                                drawing: drawing, export: export, controller: controller,
                                hasToolLock: layout == .mobile)
                        }
                    #endif
                }
                #if !os(macOS)
                    .navigationBarTitleDisplayMode(.inline)
                #endif
        }
        #if os(macOS)
            .focusedSceneValue(\.drawingExporter, DrawingExporter(drawing: drawing, export: export))
        #endif
    }

    private var canvas: some View {
        CanvasView(
            editor: drawing.editor, geometry: geometry, controller: controller,
            onSceneChange: { drawing.editorDidChange() }
        )
        .ignoresSafeArea()
        .focusedSceneValue(controller)
        .onGeometryChange(for: DrawingLayout.self) {
            DrawingLayout(containerSize: $0.size)
        } action: {
            layout = $0
        }
        .overlay {
            GeometryReader { container in
                chrome(in: container.size, bottomInset: container.safeAreaInsets.bottom)
            }
            .dynamicTypeSize(...Self.largestChromeType)
        }
        // Whatever leaves the Drawing (background, closing the window, quitting)
        // saves it at once.
        .onChange(of: scenePhase) { _, phase in
            if phase != .active { drawing.saveNow() } else { drawing.syncWithFile() }
        }
        .task { await drawing.followChangesFromOutside() }
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
    @ViewBuilder private func chrome(in size: CGSize, bottomInset: CGFloat) -> some View {
        switch DrawingLayout(containerSize: size) {
        case .desktop: desktopChrome(in: size, bottomInset: bottomInset)
        case .mobile: mobileChrome(in: size)
        }
    }

    private func desktopChrome(in size: CGSize, bottomInset: CGFloat) -> some View {
        ZStack {
            if showsProperties {
                PropertiesPanel(
                    controller: controller,
                    maxHeight: DrawingLayout.propertiesMaxHeight(containerHeight: size.height)
                )
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            }

            ZoomControls(controller: controller)
                // Sits as far from the bottom edge as from the left one: the
                // home indicator's inset (iPad) is not added to the margin.
                .offset(y: bottomInset)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottomLeading)
        }
        .padding(DrawingLayout.margin)
    }

    /// excalidraw.com's phone layout (docs/layout-reference §4): the toolbar
    /// in the system's bottom bar within thumb reach (`MobileToolbar`); no
    /// zoom buttons (pinch), properties in a sheet.
    private func mobileChrome(in size: CGSize) -> some View {
        MobileActionRow(controller: controller, showsProperties: showsProperties)
            .frame(width: DrawingLayout.bottomBarWidth(containerWidth: size.width))
            .padding(.bottom, 10)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottom)
    }

    /// The core decides when there is something to style.
    private var showsProperties: Bool { controller.properties != nil }
}

extension View {
    /// Excalidraw's 36 pt control, its hit area grown to Apple's 44 pt.
    fileprivate func controlButton(width: CGFloat = 36, height: CGFloat = 36) -> some View {
        frame(width: width, height: height)
            .contentShape(Rectangle().inset(by: -(44 - min(width, height)) / 2))
    }
}

/// Excalidraw's grey control group fill, `#ececf4`.
private let controlFill = Color(red: 0xEC / 255, green: 0xEC / 255, blue: 0xF4 / 255)
#if !os(macOS)
    /// Excalidraw's main menu in the navigation bar, as the system's own
    /// menu button: exports the Drawing. On a Mac its items are in the menu
    /// bar instead (`CanvasCommands`).
    private struct MainMenu: View {
        let drawing: OpenDrawing
        let export: ExportedDrawing
        let controller: CanvasController
        /// Whether Tool lock is here, because the toolbar has no room for it.
        let hasToolLock: Bool
        @AppStorage(ExportPreferences.backgroundKey) private var background = true

        var body: some View {
            Menu {
                if hasToolLock {
                    Toggle(
                        "Keep tool selected", systemImage: controller.toolLock ? "lock" : "lock.open",
                        isOn: Binding(get: { controller.toolLock }, set: { controller.setToolLock($0) }))
                }
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
                if offersPencilOnlyDrawing {
                    Section("Settings") {
                        Toggle(
                            isOn: Binding(
                                get: { controller.pencilOnlyDrawing }, set: { controller.setPencilOnlyDrawing($0) })
                        ) { Label("Pencil-only drawing", systemImage: "applepencil") }
                    }
                }
            } label: {
                Label("Menu", systemImage: "ellipsis")
            }
            .accessibilityIdentifier("main-menu")
        }

        private func share(_ title: LocalizedStringKey, scope: ExportScope) -> some View {
            let png = ExportedPNG(name: drawing.name, editor: drawing.editor, scope: scope, background: background)
            return ShareLink(item: png, preview: SharePreview(drawing.name)) { Label(title, systemImage: "photo") }
        }
    }
#endif

#if !os(macOS)
    /// Whether the chrome offers the Pencil-only drawing setting: on an iPad
    /// only. An iPhone has no Pencil, so the mode would leave it nothing that
    /// draws, and the core turns it on by itself only for a Pencil.
    @MainActor private var offersPencilOnlyDrawing: Bool {
        UIDevice.current.userInterfaceIdiom == .pad
    }
#endif

/// The Tools in Excalidraw's order, with their names and glyphs.
enum ToolBar {
    static func name(_ tool: Tool) -> LocalizedStringResource { tools.first { $0.0 == tool }!.1 }
    static func symbol(_ tool: Tool) -> String { tools.first { $0.0 == tool }!.2 }

    fileprivate static let tools: [(Tool, LocalizedStringResource, String)] = [
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
}

/// The desktop layout's toolbar, on the system toolbar's trailing side: Tool
/// lock, then the Tools, as toggle buttons that show the armed one.
private struct DesktopToolbar: ToolbarContent {
    let controller: CanvasController

    var body: some ToolbarContent {
        ToolbarItem(placement: .primaryAction) {
            Toggle(
                "Keep tool selected", systemImage: controller.toolLock ? "lock" : "lock.open",
                isOn: Binding(get: { controller.toolLock }, set: { controller.setToolLock($0) })
            )
            .toggleStyle(.button)
            .labelStyle(.iconOnly)
        }
        ToolbarSpacer(.fixed, placement: .primaryAction)
        ToolbarItemGroup(placement: .primaryAction) {
            ForEach(ToolBar.tools, id: \.0) { tool, name, symbol in
                Toggle(
                    isOn: Binding(get: { controller.tool == tool }, set: { _ in controller.setTool(tool) })
                ) { Label(String(localized: name), systemImage: symbol) }
                .toggleStyle(.button)
                .labelStyle(.iconOnly)
            }
        }
        ToolbarSpacer(.fixed, placement: .primaryAction)
    }
}

/// The phone toolbar (docs/layout-reference §4.2), in the system's bottom
/// bar: one slot per Tool family; tapping a family's armed Tool again offers
/// the rest of the family. Tool lock, which has no slot on excalidraw.com's
/// phone bar, is in the main menu with Pencil-only drawing (iPad only, see
/// `offersPencilOnlyDrawing`), as the bar has no room for it.
#if !os(macOS)
    private struct MobileToolbar: ToolbarContent {
        let controller: CanvasController
        /// Per slot, the Tool of its family last chosen, so the slot keeps showing it.
        @Binding var lastChosen: [Int: Tool]
        @Binding var familyShown: Int?

        var body: some ToolbarContent {
            ToolbarItem(placement: .bottomBar) {
                HStack(spacing: 0) { ForEach(MobileToolSlot.all.indices, id: \.self) { slot($0) } }
            }
        }

        private func slot(_ index: Int) -> some View {
            let slot = MobileToolSlot.all[index]
            let shown = slot.shownTool(current: controller.tool, last: lastChosen[index])
            let armed = controller.tool == shown
            return Toggle(
                isOn: Binding(
                    get: { armed },
                    set: { _ in
                        if armed && slot.tools.count > 1 {
                            familyShown = index
                        } else {
                            controller.setTool(shown)
                        }
                    })
            ) { Label(String(localized: ToolBar.name(shown)), systemImage: ToolBar.symbol(shown)) }
            .toggleStyle(.button)
            .labelStyle(.iconOnly)
            .controlSize(.small)
            .popover(
                isPresented: Binding(
                    get: { familyShown == index }, set: { if !$0 { familyShown = nil } })
            ) {
                HStack {
                    ForEach(slot.tools, id: \.self) { tool in
                        Toggle(
                            isOn: Binding(
                                get: { controller.tool == tool },
                                set: { _ in
                                    lastChosen[index] = tool
                                    controller.setTool(tool)
                                    familyShown = nil
                                })
                        ) { Label(String(localized: ToolBar.name(tool)), systemImage: ToolBar.symbol(tool)) }
                        .toggleStyle(.button)
                        .labelStyle(.iconOnly)
                    }
                }
                .padding(8)
                .presentationCompactAdaptation(.popover)
            }
        }
    }
#endif

/// The phone action row over the toolbar (docs/layout-reference §4.3):
/// properties on the left while a Tool or Selection has them, opening in a
/// sheet (the spec's native adaptation of excalidraw.com's popovers). Undo and
/// redo have no buttons: they are the system gestures (`CanvasUndoManager`).
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
        }
        .labelStyle(.iconOnly)
        .buttonStyle(.borderless)
        .tint(.primary)
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

/// Excalidraw's zoom group: out, reset (tap the percentage), in. Zoom to fit
/// is a menu command (`CanvasCommands`).
private struct ZoomControls: View {
    let controller: CanvasController

    var body: some View {
        HStack(spacing: 0) {
            Button("Zoom out", systemImage: "minus") { controller.zoomOut() }.controlButton()
            Button {
                controller.resetZoom()
            } label: {
                Text(verbatim: "\(controller.zoomPercent)%").font(.subheadline).monospacedDigit()
            }
            .controlButton(width: 60)
            .accessibilityLabel(Text("Reset zoom"))
            .accessibilityValue(Text(verbatim: "\(controller.zoomPercent)%"))
            Button("Zoom in", systemImage: "plus") { controller.zoomIn() }.controlButton()
        }
        .controlGroup()
    }
}

extension View {
    /// Excalidraw's grey bottom-left button groups.
    fileprivate func controlGroup() -> some View {
        labelStyle(.iconOnly)
            .buttonStyle(.borderless)
            .tint(.primary)
            .background(controlFill, in: RoundedRectangle(cornerRadius: 8))
    }
}
