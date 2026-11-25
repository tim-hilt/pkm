import SwiftUI
import UniformTypeIdentifiers

/// A Drawing in the Canvas with its controls, kept saved while it is open.
struct DrawingView: View {
    let drawing: OpenDrawing
    /// The Drawing as a `.excalidraw` file, for Save As.
    let export: ExportedDrawing
    let onBack: () -> Void
    @State private var controller = CanvasController()
    /// Measured so the desktop panel can tell whether it fits beside the
    /// toolbar; until then as if it did not, so the panel never overlaps it.
    @State private var toolbarWidth: CGFloat = .infinity
    /// The phone toolbar's Tool family picker, and the toolbar's top it sits above.
    @State private var family = FamilyPopoverState()
    @State private var toolbarTop: CGFloat = 0
    /// The phone Action row: its items and open popover, where its triggers are
    /// and its top, the popovers' bar.
    @State private var actionRow = ActionRowModel()
    @State private var actionAnchors: [ActionRowItem: CGRect] = [:]
    @State private var actionRowTop: CGFloat = 0
    /// The chrome's layout, which the navigation bar follows too.
    @State private var layout = DrawingLayout.desktop
    @State private var isSavingAs = false
    /// The image Export shows while it is open.
    @State private var exported: ExportedImage?
    @Environment(\.scenePhase) private var scenePhase

    private var file: DrawingFile {
        DrawingFile(
            saveAs: { isSavingAs = true },
            export: { exported = ExportedImage(drawing.editor, name: drawing.name) })
    }

    var body: some View {
        // On iPhone and iPad the system's navigation bar carries the Gallery
        // button and the main menu. A Mac's window keeps only its traffic
        // lights: going back and the menu's items are in the menu bar
        // (`CanvasCommands`). The bar shows no title: the Canvas gets the
        // screen to itself.
        NavigationStack {
            canvas
                #if !os(macOS)
                    .toolbar {
                        ToolbarItem(placement: .navigation) {
                            Button("Gallery", systemImage: "chevron.backward", action: onBack)
                            .accessibilityIdentifier("back-to-gallery")
                        }
                        // Beside the main menu, in its capsule: the phone
                        // has no room for the zoom group at the bottom.
                        if layout == .mobile {
                            ToolbarItem(placement: .primaryAction) { ZoomMenu(controller: controller) }
                        }
                        ToolbarItem(placement: .primaryAction) {
                            MainMenu(file: file, controller: controller)
                        }
                    }
                #endif
                #if os(macOS)
                    // Neither the window's title nor the faint separator
                    // under its title bar: the Canvas runs up to the top edge.
                    .toolbar(removing: .title)
                    .toolbarBackgroundVisibility(.hidden, for: .windowToolbar)
                #else
                    .navigationBarTitleDisplayMode(.inline)
                #endif
        }
        .fileExporter(
            isPresented: $isSavingAs, item: export, contentTypes: [.excalidraw],
            defaultFilename: drawing.name
        ) { _ in }
        .sheet(item: $exported) { ExportSheet(exported: $0) }
        .alert(
            "Set unit", isPresented: Binding(get: { controller.isAskingUnit }, set: { controller.isAskingUnit = $0 })
        ) {
            TextField(
                "Measure in units", text: Binding(get: { controller.unitText }, set: { controller.unitText = $0 })
            )
            #if !os(macOS)
                .keyboardType(.decimalPad)
            #endif
            Button("Set", action: controller.setUnitFromText)
                .disabled(controller.unitValue == nil)
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("What the selected width, or length of a line, reads as.")
        }
        .focusedSceneValue(\.drawingFile, file)
        #if os(macOS)
            .focusedSceneValue(\.backToGallery, onBack)
        #endif
    }

    private var canvas: some View {
        CanvasView(
            editor: drawing.editor, controller: controller,
            onSceneChange: { drawing.editorDidChange() }
        )
        .ignoresSafeArea()
        .focusedSceneValue(controller)
        .overlay {
            GeometryReader { container in
                chrome(in: container.size, bottomInset: container.safeAreaInsets.bottom)
            }
            .onGeometryChange(for: DrawingLayout.self, of: { DrawingLayout(containerSize: $0.size) }) {
                layout = $0
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
            ToolBar(controller: controller).fixedSize()
                .onGeometryChange(for: CGFloat.self, of: { $0.size.width }) { toolbarWidth = $0 }
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)

            if showsProperties {
                let top = DrawingLayout.propertiesTop(containerWidth: size.width, toolbarWidth: toolbarWidth)
                PropertiesPanel(
                    controller: controller,
                    maxHeight: DrawingLayout.propertiesMaxHeight(containerHeight: size.height, top: top)
                )
                .padding(.top, top - DrawingLayout.margin)
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

    /// excalidraw.com's phone layout (docs/layout-reference §4): everything
    /// in the bottom bar within thumb reach, properties in popovers; zoom is
    /// a menu in the navigation bar (`ZoomMenu`). The chrome sits in the keyboard's safe area
    /// (only the Canvas ignores it), so the bar rises above the keyboard with
    /// the popover anchored to it, whose maximum height is the room left above
    /// the bar (`DrawingLayout.popoverPlacement`).
    private func mobileChrome(in size: CGSize) -> some View {
        let width = DrawingLayout.bottomBarWidth(containerWidth: size.width)
        let inputs = ActionRowInputs(
            sections: controller.properties?.sections, hasSelection: controller.hasSelection, width: width)
        return VStack(spacing: 4) {
            MobileActionRow(
                controller: controller, model: actionRow, onTap: tapTrigger, anchors: $actionAnchors,
                space: Self.chromeSpace
            )
            .onGeometryChange(for: CGFloat.self, of: { $0.frame(in: .named(Self.chromeSpace)).minY }) {
                actionRowTop = $0
            }
            MobileToolBar(controller: controller, family: $family, onOpenFamily: { actionRow.close() })
                .onGeometryChange(for: CGFloat.self, of: { $0.frame(in: .named(Self.chromeSpace)).minY }) {
                    toolbarTop = $0
                }
        }
        .frame(width: width)
        .padding(.bottom, 10)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottom)
        .overlay {
            if let open = family.open, let anchor = family.anchors[open] {
                PhonePopover(anchor: anchor, container: size, barTop: toolbarTop, onClose: closePopover) {
                    FamilyPicker(controller: controller, family: $family, slot: open)
                }
            } else if let open = actionRow.open, let anchor = actionAnchors[open] {
                PhonePopover(anchor: anchor, container: size, barTop: actionRowTop, onClose: closePopover) {
                    ActionPopoverContent(item: open, model: actionRow, controller: controller)
                }
            }
        }
        .animation(.easeOut(duration: 0.15), value: family.open)
        .animation(.easeOut(duration: 0.15), value: actionRow.open)
        .popoverScrim(isOpen: family.open != nil || actionRow.open != nil, onClose: closePopover)
        .coordinateSpace(name: Self.chromeSpace)
        .onChange(of: inputs, initial: true) {
            actionRow.update(sections: inputs.sections, hasSelection: inputs.hasSelection, width: inputs.width)
        }
    }

    /// The phone chrome's coordinate space: popover anchors and bar tops are measured in it.
    fileprivate nonisolated static let chromeSpace = "chrome"

    /// Closes whichever popover is open: on a Canvas touch, which goes no
    /// further (ADR-0013), or on VoiceOver's escape gesture.
    private func closePopover() {
        family.open = nil
        _ = actionRow.canvasTouch()
    }

    private func tapTrigger(_ item: ActionRowItem) {
        family.open = nil
        actionRow.tap(item)
    }

    /// The core decides when there is something to style.
    private var showsProperties: Bool { controller.properties != nil }
}

/// Excalidraw's island: white, radius 8, its soft shadow.
private struct Island: ViewModifier {
    func body(content: Content) -> some View {
        content
            .background(Chrome.island, in: RoundedRectangle(cornerRadius: 8))
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

#if !os(macOS)
    /// Excalidraw's main menu in the navigation bar, as the system's own
    /// menu button: Save As, Export and the settings. On a Mac its items are in the menu
    /// bar instead (`CanvasCommands`), which also has their shortcuts.
    private struct MainMenu: View {
        let file: DrawingFile
        let controller: CanvasController

        var body: some View {
            Menu {
                Button("Save As…", systemImage: "square.and.arrow.down", action: file.saveAs)
                Button("Export…", systemImage: "photo", action: file.export)
                    .disabled(!controller.hasElements)
                Section("Settings") {
                    if offersPencilOnlyDrawing {
                        Toggle(
                            isOn: Binding(
                                get: { controller.pencilOnlyDrawing }, set: { controller.setPencilOnlyDrawing($0) })
                        ) { Label("Pencil-only drawing", systemImage: "applepencil") }
                    }
                    Toggle(
                        isOn: Binding(
                            get: { controller.showDimensions }, set: { controller.setShowDimensions($0) })
                    ) { Label("Show dimensions", systemImage: "ruler") }
                    if controller.showDimensions {
                        Button("Set unit…", systemImage: "scalemass", action: controller.askUnit)
                            .disabled(!controller.canSetUnit)
                        Button("Reset unit", systemImage: "arrow.counterclockwise", action: controller.resetUnit)
                            .disabled(!controller.canResetUnit)
                    }
                }
            } label: {
                Label("Menu", systemImage: "ellipsis")
            }
            .accessibilityIdentifier("main-menu")
        }
    }

    /// The phone's zoom controls: the zoom in percent, opening a menu of
    /// the zoom group's commands.
    private struct ZoomMenu: View {
        let controller: CanvasController

        var body: some View {
            Menu {
                Button("Reset", systemImage: "1.magnifyingglass") { controller.resetZoom() }
                Button("Zoom to Content", systemImage: "arrow.up.left.and.arrow.down.right") {
                    controller.zoomToFit()
                }
            } label: {
                Text(verbatim: "\(controller.zoomPercent)%").monospacedDigit()
            }
            .accessibilityLabel(Text("Zoom"))
            .accessibilityValue(Text(verbatim: "\(controller.zoomPercent)%"))
            .accessibilityIdentifier("zoom-menu")
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
        }
        .padding(4)
        .island()
    }

    private var divider: some View {
        Rectangle().fill(Chrome.divider)
            .frame(width: 1, height: 24).padding(.horizontal, 4)
    }

    private func toolButton(
        _ name: LocalizedStringResource, symbol: String, checked: Bool, action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.subheadline)
                .foregroundStyle(checked ? Chrome.checkedGlyph : .primary)
                .controlButton()
                .background(checked ? Chrome.checkedFill : .clear, in: RoundedRectangle(cornerRadius: 8))
        }
        .buttonStyle(.plain)
        .buttonPointer()
        .accessibilityLabel(Text(name))
        .accessibilityAddTraits(checked ? .isSelected : [])
    }
}

/// The phone toolbar island (docs/layout-reference §4.2): one slot per Tool
/// family, spread over the bar's width; tapping a family's armed Tool again
/// offers the rest of the family. Tool lock, which has no slot on
/// excalidraw.com's phone bar, is an icon button at the end. Pencil-only
/// drawing sits in the main menu (iPad only, see `offersPencilOnlyDrawing`).
private struct MobileToolBar: View {
    let controller: CanvasController
    @Binding var family: FamilyPopoverState
    /// Called when a family picker opens, so that no other popover stays open.
    let onOpenFamily: () -> Void

    var body: some View {
        HStack(spacing: 4) {
            ForEach(MobileToolSlot.all.indices, id: \.self) { index in
                slot(index)
                Spacer(minLength: 0)
            }
            lockButton
        }
        .padding(4)
        .island()
    }

    private func slot(_ index: Int) -> some View {
        let slot = MobileToolSlot.all[index]
        let shown = slot.shownTool(current: controller.tool, last: family.lastChosen[index])
        let armed = controller.tool == shown
        return MobileToolButton(tool: shown, checked: armed) {
            if armed && slot.tools.count > 1 {
                family.open = family.open == index ? nil : index
                onOpenFamily()
            } else {
                controller.setTool(shown)
            }
        }
        .onGeometryChange(
            for: CGRect.self, of: { $0.frame(in: .named(DrawingView.chromeSpace)) },
            action: { family.anchors[index] = $0 })
    }

    private var lockButton: some View {
        let on = controller.toolLock
        return Button {
            controller.setToolLock(!on)
        } label: {
            Image(systemName: on ? "lock" : "lock.open")
                .font(.subheadline)
                .foregroundStyle(on ? Chrome.checkedGlyph : .primary)
                .controlButton()
                .background(on ? Chrome.checkedFill : .clear, in: RoundedRectangle(cornerRadius: 4))
        }
        .buttonStyle(.plain)
        .buttonPointer()
        .accessibilityLabel("Keep tool selected")
        .accessibilityAddTraits(on ? .isSelected : [])
    }
}

/// Which Tool family picker is open on the phone toolbar, where its slots are,
/// and the Tool each family last chose.
struct FamilyPopoverState: Equatable {
    /// The open picker's slot.
    var open: Int?
    /// Per slot, its button's frame in the chrome.
    var anchors: [Int: CGRect] = [:]
    /// Per slot, the Tool of its family last chosen, so the slot keeps showing it.
    var lastChosen: [Int: Tool] = [:]
}

/// The Tools of a family, offered in a popover; choosing one arms it and closes the popover.
private struct FamilyPicker: View {
    let controller: CanvasController
    @Binding var family: FamilyPopoverState
    let slot: Int

    var body: some View {
        HStack(spacing: 4) {
            ForEach(MobileToolSlot.all[slot].tools, id: \.self) { tool in
                MobileToolButton(tool: tool, checked: controller.tool == tool) {
                    family.lastChosen[slot] = tool
                    controller.setTool(tool)
                    family.open = nil
                }
            }
        }
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
                .foregroundStyle(checked ? Chrome.checkedGlyph : .primary)
                .controlButton()
                .background(checked ? Chrome.checkedFill : .clear, in: RoundedRectangle(cornerRadius: 4))
        }
        .buttonStyle(.plain)
        .buttonPointer()
        .accessibilityLabel(Text(ToolBar.name(tool)))
        .accessibilityAddTraits(checked ? .isSelected : [])
    }
}

/// The phone Action row's inputs: a change closes a popover whose trigger is gone.
private struct ActionRowInputs: Equatable {
    let sections: Sections?
    let hasSelection: Bool
    let width: CGFloat
}

/// Excalidraw's zoom group: reset (tap the percentage) and zoom to
/// fit, which excalidraw.com has as a shortcut only.
private struct ZoomControls: View {
    let controller: CanvasController

    var body: some View {
        HStack(spacing: 0) {
            Button {
                controller.resetZoom()
            } label: {
                Text(verbatim: "\(controller.zoomPercent)%").font(.subheadline).monospacedDigit()
            }
            .controlButton(width: 60)
            .buttonPointer()
            .accessibilityLabel(Text("Reset zoom"))
            .accessibilityValue(Text(verbatim: "\(controller.zoomPercent)%"))
            Button("Zoom to fit", systemImage: "arrow.up.left.and.arrow.down.right") { controller.zoomToFit() }
                .controlButton().buttonPointer()
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
            .background(Chrome.controlFill, in: RoundedRectangle(cornerRadius: 8))
    }
}
