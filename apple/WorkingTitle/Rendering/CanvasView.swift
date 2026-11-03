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
        }
    }

    /// Called after input or a command reached the editor, for chrome that
    /// shows its state (the active Tool).
    var onEditorChange: (() -> Void)?

    /// The `Editor.revision` of the Scene the renderer shows.
    private var shownRevision: UInt64?
    /// The `Editor.overlayRevision` of the overlay the renderer shows.
    private var shownOverlayRevision: UInt64?

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
        #if !os(macOS)
            isMultipleTouchEnabled = true
        #endif
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
        onEditorChange?()
    }

    /// Hands the renderer the Scene's geometry again if the Scene changed.
    private func syncScene() {
        guard let editor, editor.revision() != shownRevision else { return }
        shownRevision = editor.revision()
        renderer?.setGeometry(editor.geometry())
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

        override func mouseDown(with event: NSEvent) { sendMouse(.down, event) }
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

        private func send(_ touches: Set<UITouch>, _ phase: PointerPhase, _ event: UIEvent?) {
            for touch in touches {
                let position = touch.location(in: self)
                send(
                    PointerEvent(
                        id: UInt64(UInt(bitPattern: ObjectIdentifier(touch).hashValue)), phase: phase,
                        x: position.x, y: position.y, kind: PointerKind(touch.type),
                        pressure: touch.maximumPossibleForce > 0
                            ? Float(touch.force / touch.maximumPossibleForce) : 0.5,
                        timestamp: touch.timestamp,
                        modifiers: Modifiers(event?.modifierFlags ?? [])))
            }
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
    /// Whether there is something to undo or redo; the buttons are enabled
    /// exactly then.
    private(set) var canUndo = false
    private(set) var canRedo = false

    private func refresh() {
        guard let editor = canvas?.editor else { return }
        tool = editor.tool()
        toolLock = editor.toolLock()
        canUndo = editor.canUndo()
        canRedo = editor.canRedo()
    }

    func zoomIn() { canvas?.perform { $0.zoomIn() } }
    func zoomOut() { canvas?.perform { $0.zoomOut() } }
    func resetZoom() { canvas?.perform { $0.resetZoom() } }
    func zoomToFit() { canvas?.perform { $0.zoomToFit() } }
    func undo() { canvas?.perform { $0.undo() } }
    func redo() { canvas?.perform { $0.redo() } }
    func setTool(_ tool: Tool) { canvas?.perform { $0.setTool(tool: tool) } }
    func setToolLock(_ locked: Bool) { canvas?.perform { $0.setToolLock(locked: locked) } }
}
