import MetalKit
import SwiftUI

/// The platform's Metal-backed view for the Canvas: all drawing lives in
/// `CanvasRenderer`; this class configures `MTKView` for on-demand rendering,
/// forwards raw input to the core's `Editor` (ADR-0006) and bridges the calls
/// that differ between UIKit and AppKit.
///
/// Panning and zooming happen in the core, but only as a new `Viewport` that
/// is handed to the renderer; the geometry stays cached in the renderer and is
/// never requested again for it (ADR-0007). Input that changes the Scene
/// (drawing) shows up as a new `Editor.revision`, and only then is the
/// geometry requested again. Likewise the overlay (selection frames) is
/// requested again only when `Editor.overlayRevision` changes.
@MainActor
final class CanvasMetalView: MTKView {
    let renderer: CanvasRenderer?

    /// Receives all input and supplies the image files. Set before input
    /// arrives; setting it shows the editor's Scene in the appearance
    /// (`isDark`) and its current viewport; changes from then on are picked
    /// up here.
    var editor: Editor? {
        didSet {
            renderer?.files = editor
            shownRevision = editor?.revision()
            showScene()
            shownOverlayRevision = nil
            hasRevealedContent = false
            reportViewSize()
            syncViewport()
            syncOverlay()
            syncTextEditor()
        }
    }

    /// Whether the system appearance is dark. The Canvas shows the Scene in
    /// Excalidraw's dark colours then (ADR-0010); changing it keeps the
    /// Selection, the Text view and the undo history.
    /// Set it before `editor`, so that a Drawing opened in dark mode is
    /// generated once, in its dark colours.
    var isDark = false {
        didSet {
            guard isDark != oldValue else { return }
            showScene()
            // The overlay and the Text view have the colours of the new
            // appearance; what is typed in the latter stays.
            syncOverlay()
            syncTextEditor()
        }
    }

    /// Hands the renderer the editor's Scene as it is now, in the colours of
    /// the appearance: its geometry and the Canvas colour.
    private func showScene() {
        guard let editor else { return }
        editor.setDarkMode(on: isDark)
        renderer?.isDark = isDark
        renderer?.setGeometry(editor.geometry())
        renderer?.paperColor = editor.canvasColor()
    }

    /// Called after input or a command reached the editor, for chrome that
    /// shows its state (the active Tool).
    var onEditorChange: (() -> Void)?

    /// Called when the Scene changed (not for panning, zooming or a Tool
    /// change), for saving it.
    var onSceneChange: (() -> Void)?

    /// The `Editor.revision` of the Scene the renderer shows.
    private var shownRevision: UInt64?
    /// The `Editor.overlayRevision` of the overlay the renderer shows.
    private var shownOverlayRevision: UInt64?
    /// The native text view over the Canvas while a text is edited.
    let textOverlay = TextEditorOverlay()
    #if !os(macOS)
        /// The system's edit menu (see `CanvasContextMenu`) and the menu it is
        /// about to show.
        var editMenuInteraction: UIEditMenuInteraction?
        var pendingContextMenu: UIMenu?
        /// Where the open menu was pressed, in the Canvas's coordinates, for a
        /// Paste from it.
        var contextMenuPoint: CGPoint?
        /// Takes the system's undo gestures to the core's history.
        private lazy var historyManager = CanvasUndoManager { [weak self] in self }
        /// Gives the pointer of a trackpad or mouse the core's cursor.
        private var pointerInteraction: UIPointerInteraction?
    #endif

    init() {
        let device = MTLCreateSystemDefaultDevice()
        renderer = device.flatMap(CanvasRenderer.init(device:))
        super.init(frame: .zero, device: device)

        // The renderer brings its own multisampled colour and stencil, in
        // tile memory only; the view provides just the drawable.
        colorPixelFormat = CanvasRenderer.pixelFormat
        // On demand only: MTKView draws when `requestRedraw` marks it dirty,
        // never from its own display-link timer.
        isPaused = true
        enableSetNeedsDisplay = true
        delegate = renderer
        renderer?.onNeedsDisplay = { [weak self] in self?.requestRedraw() }
        textOverlay.onChange = { [weak self] text in self?.perform { $0.setEditingText(text: text) } }
        textOverlay.onEnd = { [weak self] in self?.perform { $0.commitText() } }
        #if !os(macOS)
            isMultipleTouchEnabled = true
            accessibilityLabel = String(localized: "Canvas")
            enablePencilInput()
            enableContextMenu()
            observeKeyboard()
            enablePointer()
        #endif
        enableImageInput()
    }

    @available(*, unavailable)
    required init(coder: NSCoder) { fatalError("CanvasMetalView is created in code") }

    #if !os(macOS)
        override var undoManager: UndoManager? { historyManager }

        /// The Canvas is a single element for assistive technology only while
        /// it offers the Element menu's actions. While a text is edited, the
        /// text view inside must stay reachable instead.
        override var isAccessibilityElement: Bool {
            get { editor?.selection().isEmpty == false && textOverlay.view == nil }
            set { super.isAccessibilityElement = newValue }
        }

        /// The Element menu's actions while a Selection exists, for VoiceOver
        /// and Switch Control (see `CanvasAccessibilityActions`); none without
        /// one.
        override var accessibilityCustomActions: [UIAccessibilityCustomAction]? {
            get {
                let actions = accessibleActions
                guard !actions.isEmpty else { return nil }
                return actions.map { action in
                    UIAccessibilityCustomAction(name: action.name) { [weak self] _ in
                        guard let self else { return false }
                        MainActor.assumeIsolated { action.run(self) }
                        return true
                    }
                }
            }
            set { super.accessibilityCustomActions = newValue }
        }
    #endif

    // MARK: Commands

    /// Runs a command on the editor (zoom, Tool change, ...) and shows the
    /// resulting viewport.
    func perform(_ command: (Editor) -> Void) {
        guard let editor else { return }
        command(editor)
        didReachEditor()
    }

    /// A key pressed while the Canvas has the keyboard: performs the Command
    /// the core binds to it (`shortcuts()`) and tells whether there was one.
    /// Most keys arrive as menu shortcuts instead (`CanvasCommands`); this
    /// catches the others a Command has. `key` as in `Shortcut.key`.
    func pressKey(_ key: String, _ modifiers: Modifiers) -> Bool {
        guard textOverlay.view == nil, let command = MenuShortcut.command(for: key, modifiers) else {
            return false
        }
        perform { $0.perform(command: command) }
        return true
    }

    /// Reports the space bar to the core, which pans with it held. Space goes
    /// to the text view while one is open, so it is never reported then; a
    /// release is always reported, so that no key-up can be lost.
    private func setPanKey(_ held: Bool) {
        guard !held || textOverlay.view == nil else { return }
        perform { $0.setPanKey(held: held) }
    }

    /// Edit > Select All (⌘A) while the Canvas has the keyboard.
    override func selectAll(_ sender: Any?) { perform { $0.perform(command: .selectAll) } }

    /// Edit > Delete while the Canvas has the keyboard; the Backspace and
    /// Forward Delete keys reach the core through `pressKey`.
    #if os(macOS)
        @objc func delete(_ sender: Any?) { perform { $0.perform(command: .delete) } }
    #else
        override func delete(_ sender: Any?) { perform { $0.perform(command: .delete) } }
    #endif

    /// Whether the editor's content was brought into view once it had a size.
    private var hasRevealedContent = false

    /// The core's zoom commands act around the middle of the view, so it must
    /// know the view's size. With the first size an opened Drawing shows its
    /// content, even if it was left (or made elsewhere) with none in view.
    private func reportViewSize() {
        guard let editor else { return }
        editor.setViewSize(width: bounds.width, height: bounds.height)
        guard !hasRevealedContent, !bounds.isEmpty else { return }
        hasRevealedContent = true
        editor.revealContent()
        syncViewport()
        syncOverlay()
        onEditorChange?()
    }

    #if os(macOS)
        override func layout() {
            super.layout()
            reportViewSize()
        }
    #else
        override func layoutSubviews() {
            super.layoutSubviews()
            reportViewSize()
        }
    #endif

    // MARK: Input

    func send(_ event: PointerEvent) {
        editor?.pointer(event: event)
        didReachEditor()
    }

    func send(_ event: ScrollEvent) {
        editor?.scroll(event: event)
        didReachEditor()
    }

    func send(_ event: MagnifyEvent) {
        editor?.magnify(event: event)
        didReachEditor()
    }

    /// Shows whatever the editor did with an input or command.
    private func didReachEditor() {
        syncScene()
        syncViewport()
        syncOverlay()
        syncTextEditor()
        syncCursor()
        onEditorChange?()
    }

    /// Shows the core's cursor for where the mouse is: what it does may have
    /// changed with any input or command (a Tool, the space bar, a drag).
    private func syncCursor() {
        #if os(macOS)
            guard let editor, let window, let contentView = window.contentView else { return }
            let inWindow = window.mouseLocationOutsideOfEventStream
            // Over the chrome or the text view, those show their own cursors;
            // a drag that began on the Canvas keeps its cursor everywhere.
            let hit = contentView.hitTest(contentView.superview?.convert(inWindow, from: nil) ?? inWindow)
            guard hit === self || isMouseDragging else { return }
            let at = convert(inWindow, from: nil)
            editor.cursor(x: at.x, y: at.y).nsCursor.set()
        #else
            pointerInteraction?.invalidate()
        #endif
    }

    /// Shows the text view where the core says, or takes it away if no text
    /// is edited. Like the viewport it follows panning and zooming.
    private func syncTextEditor() {
        if let shown = editor?.textEditor() {
            textOverlay.show(shown, in: self)
            #if !os(macOS)
                avoidKeyboard()
            #endif
        } else {
            textOverlay.hide()
        }
    }

    #if !os(macOS)
        /// The on-screen keyboard's frame in screen coordinates; `nil` while
        /// it is hidden. A hardware keyboard shows none (or only the small
        /// shortcut bar, which the caret never needs to clear).
        private var keyboardFrame: CGRect?

        private func observeKeyboard() {
            NotificationCenter.default.addObserver(
                self, selector: #selector(keyboardWillChangeFrame(_:)),
                name: UIResponder.keyboardWillChangeFrameNotification, object: nil)
        }

        @objc private func keyboardWillChangeFrame(_ note: Notification) {
            keyboardFrame = (note.userInfo?[UIResponder.keyboardFrameEndUserInfoKey] as? NSValue)?.cgRectValue
            avoidKeyboard()
        }

        /// Pans the viewport, if the keyboard covers the caret's line, so that
        /// the text is in the middle of the room above the keyboard, below
        /// the status bar. Does not pan back afterwards.
        private func avoidKeyboard() {
            guard let editor, let field = textOverlay.view, let window,
                let end = field.selectedTextRange?.end
            else { return }
            let keyboard = keyboardFrame.map { window.screen.coordinateSpace.convert($0, to: self) }
            let caret = field.convert(field.caretRect(for: end), to: self)
            guard
                let panned = KeyboardAvoidance.viewport(
                    keyboard: keyboard, top: safeAreaInsets.top, text: field.frame, caret: caret,
                    current: editor.viewport())
            else { return }
            editor.setViewport(viewport: panned)
            syncViewport()
            syncOverlay()
            if let moved = editor.textEditor() { textOverlay.show(moved, in: self) }
            onEditorChange?()
        }
    #endif

    /// Hands the renderer the Scene's geometry again if the Scene changed.
    private func syncScene() {
        guard let editor, editor.revision() != shownRevision else { return }
        shownRevision = editor.revision()
        showScene()
        onSceneChange?()
    }

    /// Hands the renderer the overlay again if it may have changed: with the
    /// Selection, the Elements selected and the zoom, but not with a pan.
    private func syncOverlay() {
        guard let editor, editor.overlayRevision() != shownOverlayRevision else { return }
        shownOverlayRevision = editor.overlayRevision()
        renderer?.setOverlay(editor.overlay())
    }

    /// Shows what the core says is in view. Cheap: the renderer ignores an
    /// unchanged viewport.
    private func syncViewport() {
        guard let editor else { return }
        renderer?.viewport = Viewport(editor.viewport())
    }

    #if os(macOS)
        // AppKit's origin is bottom-left; the core expects y down.
        override var isFlipped: Bool { true }

        override func scrollWheel(with event: NSEvent) {
            // Wheel ticks come in lines; the core wants points.
            let scale: CGFloat = event.hasPreciseScrollingDeltas ? 1 : Self.pointsPerWheelLine
            let position = viewPoint(of: event)
            send(
                ScrollEvent(
                    x: position.x, y: position.y,
                    dx: event.scrollingDeltaX * scale, dy: event.scrollingDeltaY * scale,
                    modifiers: Modifiers(event.modifierFlags)))
        }

        override func magnify(with event: NSEvent) {
            let position = viewPoint(of: event)
            send(MagnifyEvent(x: position.x, y: position.y, magnification: event.magnification))
        }

        override var acceptsFirstResponder: Bool { true }

        // MARK: Cursor

        /// Whether a mouse button is down on the Canvas: its drag keeps the
        /// cursor, even over the chrome.
        private var isMouseDragging = false

        /// Mouse moves only, no cursor updates: the Canvas lies under the
        /// whole window, and its cursor-update area would take the updates
        /// the chrome's buttons show their pointing hand with.
        override func updateTrackingAreas() {
            super.updateTrackingAreas()
            if trackingAreas.isEmpty {
                addTrackingArea(
                    NSTrackingArea(
                        rect: .zero, options: [.mouseMoved, .activeInKeyWindow, .inVisibleRect], owner: self))
            }
        }

        override func mouseMoved(with event: NSEvent) { syncCursor() }

        /// A lost key-up must not leave panning stuck.
        override func resignFirstResponder() -> Bool {
            setPanKey(false)
            return super.resignFirstResponder()
        }

        /// Keys the menus do not list (see `pressKey`): Escape finishes a
        /// line being placed; Enter does too, and otherwise edits the text
        /// of the Selection, as on excalidraw.com. The core decides.
        override func keyDown(with event: NSEvent) {
            if event.keyCode == Self.spaceKey, textOverlay.view == nil {
                setPanKey(true)
                return
            }
            if !pressKey(Self.keyName(of: event), Modifiers(event.modifierFlags)) { super.keyDown(with: event) }
        }

        override func keyUp(with event: NSEvent) {
            if event.keyCode == Self.spaceKey { setPanKey(false) } else { super.keyUp(with: event) }
        }

        /// The key as `Shortcut.key` names it: the character it types without
        /// any modifier. `charactersIgnoringModifiers` keeps Shift, so ⇧1
        /// would read "!" and never reach zoom to fit.
        static func keyName(of event: NSEvent) -> String {
            switch event.keyCode {
            case backspaceKey: "backspace"
            case forwardDeleteKey: "delete"
            case escapeKey: "escape"
            case returnKey, keypadEnterKey: "enter"
            case leftArrowKey: "arrowleft"
            case rightArrowKey: "arrowright"
            case downArrowKey: "arrowdown"
            case upArrowKey: "arrowup"
            default: event.characters(byApplyingModifiers: [])?.lowercased() ?? ""
            }
        }

        private static let spaceKey: UInt16 = 49
        private static let backspaceKey: UInt16 = 51
        private static let forwardDeleteKey: UInt16 = 117

        private static let escapeKey: UInt16 = 53
        private static let returnKey: UInt16 = 36
        private static let keypadEnterKey: UInt16 = 76
        private static let leftArrowKey: UInt16 = 123
        private static let rightArrowKey: UInt16 = 124
        private static let downArrowKey: UInt16 = 125
        private static let upArrowKey: UInt16 = 126

        override func mouseDown(with event: NSEvent) {
            // Keys go to the Canvas, unless a text is being edited: the core
            // commits that on the click.
            if textOverlay.view == nil { window?.makeFirstResponder(self) }
            sendMouse(.down, event)
        }

        override func mouseDragged(with event: NSEvent) { sendMouse(.move, event) }
        override func mouseUp(with event: NSEvent) { sendMouse(.up, event) }

        // The right button and the wheel button pan; the core tells a
        // right-click from a pan.
        override func rightMouseDown(with event: NSEvent) { mouseDown(with: event) }
        override func rightMouseDragged(with event: NSEvent) { mouseDragged(with: event) }
        override func rightMouseUp(with event: NSEvent) { mouseUp(with: event) }
        override func otherMouseDown(with event: NSEvent) { mouseDown(with: event) }
        override func otherMouseDragged(with event: NSEvent) { mouseDragged(with: event) }
        override func otherMouseUp(with event: NSEvent) { mouseUp(with: event) }

        private static let pointsPerWheelLine: CGFloat = 10

        private func viewPoint(of event: NSEvent) -> CGPoint {
            convert(event.locationInWindow, from: nil)
        }

        /// The button of a mouse event; buttons beyond the wheel button are
        /// the primary one's.
        private func button(of event: NSEvent) -> PointerButton {
            switch event.type {
            case .rightMouseDown, .rightMouseDragged, .rightMouseUp: .secondary
            case .otherMouseDown, .otherMouseDragged, .otherMouseUp:
                event.buttonNumber == 2 ? .middle : .primary
            default: .primary
            }
        }

        private func sendMouse(_ phase: PointerPhase, _ event: NSEvent) {
            let position = viewPoint(of: event)
            isMouseDragging = phase == .down || phase == .move
            send(
                PointerEvent(
                    id: 0, phase: phase, x: position.x, y: position.y, kind: .mouse,
                    button: button(of: event), pressure: 0.5, timestamp: event.timestamp,
                    modifiers: Modifiers(event.modifierFlags)))
        }
    #else
        private func enablePointer() {
            let interaction = UIPointerInteraction(delegate: self)
            addInteraction(interaction)
            pointerInteraction = interaction
        }

        /// Keys the menus do not list (see `pressKey`).
        override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
            var unhandled = Set<UIPress>()
            for press in presses {
                guard let key = press.key else {
                    unhandled.insert(press)
                    continue
                }
                if key.keyCode == .keyboardSpacebar, textOverlay.view == nil {
                    setPanKey(true)
                    continue
                }
                let name =
                    switch key.keyCode {
                    case .keyboardDeleteOrBackspace: "backspace"
                    case .keyboardDeleteForward: "delete"
                    case .keyboardEscape: "escape"
                    case .keyboardReturnOrEnter, .keypadEnter: "enter"
                    case .keyboardLeftArrow: "arrowleft"
                    case .keyboardRightArrow: "arrowright"
                    case .keyboardUpArrow: "arrowup"
                    case .keyboardDownArrow: "arrowdown"
                    default: key.charactersIgnoringModifiers.lowercased()
                    }
                if !pressKey(name, Modifiers(key.modifierFlags)) { unhandled.insert(press) }
            }
            if !unhandled.isEmpty { super.pressesBegan(unhandled, with: event) }
        }

        override func pressesEnded(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
            releaseSpace(presses, with: event) { super.pressesEnded($0, with: event) }
        }

        override func pressesCancelled(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
            releaseSpace(presses, with: event) { super.pressesCancelled($0, with: event) }
        }

        /// Reports the release of space; the other presses go on to `rest`.
        private func releaseSpace(_ presses: Set<UIPress>, with event: UIPressesEvent?, rest: (Set<UIPress>) -> Void) {
            let others = presses.filter { $0.key?.keyCode != .keyboardSpacebar }
            if others.count != presses.count { setPanKey(false) }
            if !others.isEmpty { rest(others) }
        }

        /// A lost key-up must not leave panning stuck.
        override func resignFirstResponder() -> Bool {
            setPanKey(false)
            return super.resignFirstResponder()
        }

        override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent?) { send(touches, .down, event) }
        override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent?) { send(touches, .move, event) }
        override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent?) { send(touches, .up, event) }
        override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent?) {
            send(touches, .cancel, event)
        }

        /// Forwards the touches, and shows what the editor did once. A moving
        /// touch is forwarded with every sample the system coalesced into it
        /// (the Pencil samples faster than the display refreshes), so that
        /// strokes follow it closely.
        private func send(_ touches: Set<UITouch>, _ phase: PointerPhase, _ event: UIEvent?) {
            // Keys (⌘V) go to the Canvas, unless a text is being edited.
            if phase == .down && textOverlay.view == nil { becomeFirstResponder() }
            let modifiers = Modifiers(event?.modifierFlags ?? [])
            for touch in touches {
                let id = UInt64(UInt(bitPattern: ObjectIdentifier(touch).hashValue))
                let samples = phase == .move ? event?.coalescedTouches(for: touch) ?? [touch] : [touch]
                for sample in samples {
                    let position = sample.location(in: self)
                    editor?.pointer(
                        event: PointerEvent(
                            id: id, phase: phase, x: position.x, y: position.y,
                            kind: PointerKind(sample.type),
                            button: sample.type == .indirectPointer && event?.buttonMask.contains(.secondary) == true
                                ? .secondary : .primary,
                            pressure: sample.maximumPossibleForce > 0
                                ? Float(sample.force / sample.maximumPossibleForce) : 0.5,
                            timestamp: sample.timestamp, modifiers: modifiers))
                }
            }
            didReachEditor()
        }
    #endif

    /// Schedules exactly one redraw for the next display cycle; repeated
    /// requests before then coalesce.
    func requestRedraw() {
        #if os(macOS)
            needsDisplay = true
        #else
            setNeedsDisplay()
        #endif
    }
}

#if !os(macOS)
    extension CanvasMetalView: UIPointerInteractionDelegate {
        /// A region of a single point around the pointer, so that the system
        /// asks again as soon as it moves, with the core's cursor there.
        func pointerInteraction(
            _ interaction: UIPointerInteraction, regionFor request: UIPointerRegionRequest,
            defaultRegion: UIPointerRegion
        ) -> UIPointerRegion? {
            let at = request.location
            let cursor = editor?.cursor(x: at.x, y: at.y) ?? .arrow
            return UIPointerRegion(rect: CGRect(x: at.x - 0.5, y: at.y - 0.5, width: 1, height: 1), identifier: cursor)
        }

        func pointerInteraction(_ interaction: UIPointerInteraction, styleFor region: UIPointerRegion)
            -> UIPointerStyle?
        {
            (region.identifier as? Cursor)?.pointerStyle
        }
    }
#endif

/// SwiftUI host for `CanvasMetalView`: the thin platform layer, identical on
/// iOS and macOS except for the representable protocol it adopts.
struct CanvasView {
    let editor: Editor
    let controller: CanvasController
    /// Called when the Scene changed.
    var onSceneChange: (() -> Void)?

    @MainActor private func makeCanvas(isDark: Bool) -> CanvasMetalView {
        let canvas = CanvasMetalView()
        update(canvas, isDark: isDark)
        return canvas
    }

    @MainActor private func update(_ canvas: CanvasMetalView, isDark: Bool) {
        canvas.isDark = isDark
        // SwiftUI updates this view for reasons that have nothing to do with
        // the Scene (a chrome state change); the Canvas is not reset then.
        if canvas.editor !== editor { canvas.editor = editor }
        canvas.onSceneChange = onSceneChange
        controller.canvas = canvas
    }
}

#if os(macOS)
    extension CanvasView: NSViewRepresentable {
        func makeNSView(context: Context) -> CanvasMetalView {
            makeCanvas(isDark: context.environment.colorScheme == .dark)
        }
        func updateNSView(_ canvas: CanvasMetalView, context: Context) {
            update(canvas, isDark: context.environment.colorScheme == .dark)
        }
    }
#else
    extension CanvasView: UIViewRepresentable {
        func makeUIView(context: Context) -> CanvasMetalView {
            makeCanvas(isDark: context.environment.colorScheme == .dark)
        }
        func updateUIView(_ canvas: CanvasMetalView, context: Context) {
            update(canvas, isDark: context.environment.colorScheme == .dark)
        }
    }
#endif

/// Lets chrome outside the Canvas (toolbar, buttons) command it, and shows it
/// the editor's state. Commands reach the core's `Editor` and the Canvas shows
/// the result.
@MainActor @Observable
final class CanvasController {
    @ObservationIgnored weak var canvas: CanvasMetalView? {
        didSet {
            canvas?.onEditorChange = { [weak self] in self?.refresh() }
            refresh()
        }
    }

    /// The active Tool and whether it is locked, as the editor has them. The
    /// editor changes the Tool itself, e.g. back to selection after drawing.
    private(set) var tool = Tool.selection
    private(set) var toolLock = false
    /// Whether a text is being edited: keys then type into it.
    private(set) var isEditingText = false
    /// Whether Pencil-only drawing is on. The core turns it on with the first
    /// Pencil input; it is stored in `settings` once it is on or the user
    /// chose, and restored into each editor before it is first read.
    private(set) var pencilOnlyDrawing = false
    @ObservationIgnored private var restoredEditor: Editor?
    @ObservationIgnored private let settings: UserDefaults
    private static let pencilOnlyKey = "pencilOnlyDrawing"
    /// Whether there is something to undo or redo.
    private(set) var canUndo = false
    private(set) var canRedo = false
    /// What the properties panel shows: the Selection's values, or the style
    /// of the next Element if nothing is selected. `nil` while the panel is
    /// hidden (`Editor.showsProperties`) and until there is an editor.
    private(set) var properties: Properties?
    /// Whether something is selected: it enables the Edit and Export Selection
    /// items exactly then.
    private(set) var hasSelection = false
    /// The zoom in whole percent, as the reset zoom button shows it. Whole
    /// percent, so that a pinch updates the chrome only when the number does.
    private(set) var zoomPercent = 100

    /// `settings` is where the Pencil-only choice is kept; tests pass their own.
    init(settings: UserDefaults = .standard) {
        self.settings = settings
    }

    private func refresh() {
        guard let editor = canvas?.editor else { return }
        tool = editor.tool()
        toolLock = editor.toolLock()
        isEditingText = editor.textEditor() != nil
        syncPencilOnlyDrawing(editor)
        canUndo = editor.canUndo()
        canRedo = editor.canRedo()
        properties = editor.showsProperties() ? editor.properties() : nil
        hasSelection = !editor.selection().isEmpty
        zoomPercent = Int((editor.viewport().zoom * 100).rounded())
    }

    private func syncPencilOnlyDrawing(_ editor: Editor) {
        let stored = settings.object(forKey: Self.pencilOnlyKey) as? Bool
        if restoredEditor !== editor {
            restoredEditor = editor
            // Only a stored setting is a decision; without one the core decides.
            if let stored { editor.setPencilOnlyDrawing(on: stored) }
        }
        pencilOnlyDrawing = editor.pencilOnlyDrawing()
        // The core turned it on by itself: keep that for the next launch.
        if pencilOnlyDrawing != (stored ?? false) {
            settings.set(pencilOnlyDrawing, forKey: Self.pencilOnlyKey)
        }
    }

    func zoomIn() { canvas?.perform { $0.zoomIn() } }
    func zoomOut() { canvas?.perform { $0.zoomOut() } }
    func resetZoom() { canvas?.perform { $0.resetZoom() } }
    func zoomToFit() { canvas?.perform { $0.zoomToFit() } }
    func undo() { canvas?.perform { $0.undo() } }
    func redo() { canvas?.perform { $0.redo() } }
    func setTool(_ tool: Tool) { canvas?.perform { $0.setTool(tool: tool) } }
    func setToolLock(_ locked: Bool) { canvas?.perform { $0.setToolLock(locked: locked) } }
    /// The user's choice in settings; stored even when off, so that a Pencil
    /// no longer turns it on by itself.
    func setPencilOnlyDrawing(_ on: Bool) {
        settings.set(on, forKey: Self.pencilOnlyKey)
        canvas?.perform { $0.setPencilOnlyDrawing(on: on) }
    }
    func perform(_ command: Command) { canvas?.perform { $0.perform(command: command) } }
    func duplicate() { canvas?.perform { $0.duplicate() } }
    func group() { canvas?.perform { $0.group() } }
    func ungroup() { canvas?.perform { $0.ungroup() } }
    func moveInZOrder(_ how: ZOrder) { canvas?.perform { $0.moveInZOrder(how: how) } }
    func align(_ how: Alignment) { canvas?.perform { $0.align(how: how) } }
    func distribute(_ how: Distribution) { canvas?.perform { $0.distribute(how: how) } }
    func deleteSelection() { canvas?.perform { $0.deleteSelection() } }
    /// Puts the Selection on the system pasteboard, as Edit > Copy does.
    func copySelection() { canvas?.copy(nil) }
    /// Puts the Selection on the system pasteboard and deletes it, as Edit > Cut does.
    func cutSelection() { canvas?.cut(nil) }
    func setProperty(_ property: Property) { canvas?.perform { $0.setProperty(property: property) } }
}
