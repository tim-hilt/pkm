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
    /// arrives; setting it shows the editor's current viewport. The renderer
    /// is expected to show the editor's Scene as it is when this is set (see
    /// `CanvasRenderer.setGeometry`); changes from then on are picked up here.
    var editor: Editor? {
        didSet {
            renderer?.files = editor
            shownRevision = editor?.revision()
            shownOverlayRevision = nil
            reportViewSize()
            syncViewport()
            syncOverlay()
            syncTextEditor()
        }
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

    init() {
        let device = MTLCreateSystemDefaultDevice()
        renderer = device.flatMap(CanvasRenderer.init(device:))
        super.init(frame: .zero, device: device)

        colorPixelFormat = CanvasRenderer.pixelFormat
        sampleCount = CanvasRenderer.sampleCount
        depthStencilPixelFormat = CanvasRenderer.stencilFormat
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
            enablePencilInput()
        #endif
        enableImageInput()
    }

    @available(*, unavailable)
    required init(coder: NSCoder) { fatalError("CanvasMetalView is created in code") }

    // MARK: Commands

    /// Runs a command on the editor (zoom, Tool change, ...) and shows the
    /// resulting viewport.
    func perform(_ command: (Editor) -> Void) {
        guard let editor else { return }
        command(editor)
        didReachEditor()
    }

    /// The core's zoom commands act around the middle of the view, so it must
    /// know the view's size.
    private func reportViewSize() {
        editor?.setViewSize(width: bounds.width, height: bounds.height)
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
        onEditorChange?()
    }

    /// Shows the text view where the core says, or takes it away if no text
    /// is edited. Like the viewport it follows panning and zooming.
    private func syncTextEditor() {
        if let shown = editor?.textEditor() {
            textOverlay.show(shown, in: self)
        } else {
            textOverlay.hide()
        }
    }

    /// Hands the renderer the Scene's geometry again if the Scene changed.
    private func syncScene() {
        guard let editor, editor.revision() != shownRevision else { return }
        shownRevision = editor.revision()
        renderer?.setGeometry(editor.geometry())
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

        /// Enter edits the text of the selected shape or text, as on
        /// excalidraw.com; the core decides what that is.
        override func keyDown(with event: NSEvent) {
            switch event.keyCode {
            case Self.returnKey, Self.keypadEnterKey: perform { $0.editSelectedText() }
            default: super.keyDown(with: event)
            }
        }

        private static let returnKey: UInt16 = 36
        private static let keypadEnterKey: UInt16 = 76

        override func mouseDown(with event: NSEvent) {
            // Keys go to the Canvas, unless a text is being edited: the core
            // commits that on the click.
            if textOverlay.view == nil { window?.makeFirstResponder(self) }
            sendMouse(.down, event)
        }

        override func mouseDragged(with event: NSEvent) { sendMouse(.move, event) }
        override func mouseUp(with event: NSEvent) { sendMouse(.up, event) }

        private static let pointsPerWheelLine: CGFloat = 10

        private func viewPoint(of event: NSEvent) -> CGPoint {
            convert(event.locationInWindow, from: nil)
        }

        private func sendMouse(_ phase: PointerPhase, _ event: NSEvent) {
            let position = viewPoint(of: event)
            send(
                PointerEvent(
                    id: 0, phase: phase, x: position.x, y: position.y, kind: .mouse,
                    pressure: 0.5, timestamp: event.timestamp, modifiers: Modifiers(event.modifierFlags)))
        }
    #else
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

/// SwiftUI host for `CanvasMetalView`: the thin platform layer, identical on
/// iOS and macOS except for the representable protocol it adopts.
struct CanvasView {
    let editor: Editor
    /// The editor's geometry when it is handed over; the Canvas keeps it up
    /// to date from then on.
    let geometry: Geometry
    let controller: CanvasController
    /// Called when the Scene changed.
    var onSceneChange: (() -> Void)?

    @MainActor private func makeCanvas() -> CanvasMetalView {
        let canvas = CanvasMetalView()
        update(canvas)
        return canvas
    }

    @MainActor private func update(_ canvas: CanvasMetalView) {
        // SwiftUI updates this view for reasons that have nothing to do with
        // the Scene (a chrome state change); the Canvas must not be reset to
        // the geometry of the hand-over then.
        if canvas.editor !== editor {
            canvas.renderer?.setGeometry(geometry)
            canvas.editor = editor
        }
        canvas.onSceneChange = onSceneChange
        controller.canvas = canvas
    }
}

#if os(macOS)
    extension CanvasView: NSViewRepresentable {
        func makeNSView(context: Context) -> CanvasMetalView { makeCanvas() }
        func updateNSView(_ canvas: CanvasMetalView, context: Context) { update(canvas) }
    }
#else
    extension CanvasView: UIViewRepresentable {
        func makeUIView(context: Context) -> CanvasMetalView { makeCanvas() }
        func updateUIView(_ canvas: CanvasMetalView, context: Context) { update(canvas) }
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
    /// Whether Pencil-only drawing is on. The core turns it on with the first
    /// Pencil input; it is stored in `settings` once it is on or the user
    /// chose, and restored into each editor before it is first read.
    private(set) var pencilOnlyDrawing = false
    @ObservationIgnored private var restoredEditor: Editor?
    @ObservationIgnored private let settings = UserDefaults.standard
    private static let pencilOnlyKey = "pencilOnlyDrawing"
    /// Whether there is something to undo or redo; the buttons are enabled
    /// exactly then.
    private(set) var canUndo = false
    private(set) var canRedo = false
    /// What the properties panel shows: the Selection's values, or the style
    /// of the next Element if nothing is selected. `nil` until there is an
    /// editor.
    private(set) var properties: Properties?
    /// Where the Action Bar is shown, in the Canvas's coordinates, or `nil` if
    /// it is hidden; the core decides, and the position follows the Selection
    /// through panning and zooming.
    private(set) var actionBar: ActionBar?
    /// The size of the Canvas, which keeps the Action Bar inside it.
    private(set) var viewSize = CGSize.zero

    private func refresh() {
        guard let editor = canvas?.editor else { return }
        tool = editor.tool()
        toolLock = editor.toolLock()
        syncPencilOnlyDrawing(editor)
        canUndo = editor.canUndo()
        canRedo = editor.canRedo()
        properties = editor.properties()
        actionBar = editor.actionBar()
        viewSize = canvas?.bounds.size ?? .zero
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
    func duplicate() { canvas?.perform { $0.duplicate() } }
    func group() { canvas?.perform { $0.group() } }
    func ungroup() { canvas?.perform { $0.ungroup() } }
    func moveInZOrder(_ how: ZOrder) { canvas?.perform { $0.moveInZOrder(how: how) } }
    func deleteSelection() { canvas?.perform { $0.deleteSelection() } }
    func setProperty(_ property: Property) { canvas?.perform { $0.setProperty(property: property) } }
}
