import SwiftUI

/// A Command as the menus show it: the first of the core's shortcuts for it
/// (`shortcuts()`), as a SwiftUI keyboard shortcut. The menus make the
/// shortcuts discoverable (Mac menu bar, iPad keyboard shortcut overlay);
/// the other keys of a Command reach the core through
/// `CanvasMetalView.pressKey`.
struct MenuShortcut {
    let command: Command
    let key: KeyEquivalent
    let modifiers: EventModifiers

    /// One per Command, in the core's order.
    static let all: [MenuShortcut] = {
        var seen: [Command] = []
        return shortcuts().compactMap { shortcut in
            guard !seen.contains(shortcut.command) else { return nil }
            seen.append(shortcut.command)
            return MenuShortcut(shortcut)
        }
    }()

    private init(_ shortcut: Shortcut) {
        command = shortcut.command
        key =
            switch shortcut.key {
            case "backspace": .delete
            case "delete": .deleteForward
            case "escape": .escape
            case "enter": .return
            case "arrowleft": .leftArrow
            case "arrowright": .rightArrow
            case "arrowup": .upArrow
            case "arrowdown": .downArrow
            default: KeyEquivalent(Character(shortcut.key))
            }
        var modifiers: EventModifiers = []
        if shortcut.modifiers.meta { modifiers.insert(.command) }
        if shortcut.modifiers.shift { modifiers.insert(.shift) }
        if shortcut.modifiers.alt { modifiers.insert(.option) }
        if shortcut.modifiers.ctrl { modifiers.insert(.control) }
        self.modifiers = modifiers
    }

    /// The Command bound to `key` (see `Shortcut.key`) with `modifiers`.
    static func command(for key: String, _ modifiers: Modifiers) -> Command? {
        bindings.first { $0.key == key && $0.modifiers == modifiers }?.command
    }

    private static let bindings = shortcuts()
}

/// The Canvas's Commands in the app's menus, for the Drawing in front.
struct CanvasCommands: Commands {
    @FocusedValue(CanvasController.self) private var controller
    #if os(macOS)
        @FocusedValue(\.drawingExporter) private var exporter
        @FocusedValue(\.backToGallery) private var backToGallery
    #endif

    var body: some Commands {
        // A Mac has no main menu button on the Canvas: its items are here.
        #if os(macOS)
            // The Canvas has no Gallery button on a Mac: ⌘W, which would
            // close the app's only window, leaves the Drawing instead.
            CommandGroup(replacing: .saveItem) {
                if let backToGallery {
                    Button("Back to Gallery", action: backToGallery)
                        .keyboardShortcut("w")
                } else {
                    Button("Close") { NSApp.keyWindow?.performClose(nil) }
                        .keyboardShortcut("w")
                }
            }
            CommandGroup(replacing: .importExport) {
                Button("Export…") { exporter?.exportDrawing() }
                    .disabled(exporter == nil)
                Menu("Export PNG") {
                    Button("Export Scene as PNG…") {
                        exporter?.exportPNG(scope: .scene)
                    }
                    Button("Export Selection as PNG…") {
                        exporter?.exportPNG(scope: .selection)
                    }
                    .disabled(controller?.hasSelection != true)
                }
                .disabled(exporter == nil)
            }
        #endif
        CommandGroup(replacing: .undoRedo) {
            items(for: [.undo, .redo])
        }
        // Select All and Delete stay the platform's own Edit items (see
        // `CanvasMetalView.selectAll` and `delete`), so that they are not
        // listed twice.
        CommandGroup(after: .pasteboard) {
            // The client's own, not the core's: like Copy, it goes to the
            // pasteboard. ⇧⌥C as on excalidraw.com.
            Button("Copy as PNG") { controller?.canvas?.copyAsPNG() }
                .keyboardShortcut("c", modifiers: [.shift, .option])
                .disabled(controller == nil || controller?.isEditingText == true)
            Divider()
            items(for: [.duplicate])
        }
        CommandGroup(after: .toolbar) {
            items(for: [.zoomIn, .zoomOut, .resetZoom, .zoomToFit])
        }
        CommandMenu("Tools") {
            items { if case .setTool = $0 { true } else { false } }
            Divider()
            items(for: [.toggleToolLock])
        }
        CommandMenu("Arrange") {
            items(for: [.group, .ungroup])
            Divider()
            items { if case .moveInZOrder = $0 { true } else { false } }
            Divider()
            items { if case .align = $0 { true } else { false } }
            Divider()
            items { if case .distribute = $0 { true } else { false } }
        }
    }

    private func items(for commands: [Command]) -> some View {
        items { commands.contains($0) }
    }

    private func items(where included: @escaping (Command) -> Bool) -> some View {
        ForEach(Array(MenuShortcut.all.enumerated()), id: \.offset) { _, entry in
            if included(entry.command) {
                Button(Self.title(entry.command)) { controller?.perform(entry.command) }
                    .keyboardShortcut(entry.key, modifiers: entry.modifiers)
                    // Keys type into a text that is being edited.
                    .disabled(controller == nil || controller?.isEditingText == true)
            }
        }
    }

    static func title(_ command: Command) -> LocalizedStringResource {
        switch command {
        case .setTool(let tool): ToolBar.name(tool)
        case .toggleToolLock: "Keep Tool Selected"
        case .undo: "Undo"
        case .redo: "Redo"
        case .duplicate: "Duplicate"
        case .delete: "Delete"
        case .group: "Group Selection"
        case .ungroup: "Ungroup Selection"
        case .moveInZOrder(.forward): "Bring Forward"
        case .moveInZOrder(.backward): "Send Backward"
        case .moveInZOrder(.toFront): "Bring to Front"
        case .moveInZOrder(.toBack): "Send to Back"
        case .align(.left): "Align Left"
        case .align(.centerHorizontally): "Center Horizontally"
        case .align(.right): "Align Right"
        case .align(.top): "Align Top"
        case .align(.centerVertically): "Center Vertically"
        case .align(.bottom): "Align Bottom"
        case .distribute(.horizontally): "Distribute Horizontally"
        case .distribute(.vertically): "Distribute Vertically"
        case .selectAll: "Select All"
        case .zoomIn: "Zoom In"
        case .zoomOut: "Zoom Out"
        case .resetZoom: "Reset Zoom"
        case .zoomToFit: "Zoom to Fit"
        case .finishLine: "Finish Line"
        case .finishLineOrEditText: "Finish Line or Edit Text"
        }
    }
}

#if os(macOS)
    extension FocusedValues {
        /// Leaves the Drawing in front for the Gallery (File > Back to Gallery).
        @Entry var backToGallery: (() -> Void)?
    }
#endif
