import MetalKit
import SwiftUI

/// The platform's Metal-backed view for the Canvas: all drawing lives in
/// `CanvasRenderer`; this class configures `MTKView` for on-demand rendering,
/// forwards raw input to the core's `Editor` (ADR-0006) and bridges the calls
/// that differ between UIKit and AppKit.
///
/// Panning and zooming happen in the core, but only as a new `Viewport` that
/// is handed to the renderer; the geometry stays cached in the renderer and is
/// never requested again for it (ADR-0007).
@MainActor
final class CanvasMetalView: MTKView {
    let renderer: CanvasRenderer?

    /// Receives all input. Set before input arrives; setting it shows the
    /// editor's current viewport.
    var editor: Editor? {
        didSet {
            reportViewSize()
            syncViewport()
        }
    }

    init() {
        let device = MTLCreateSystemDefaultDevice()
        renderer = device.flatMap(CanvasRenderer.init(device:))
        super.init(frame: .zero, device: device)

        colorPixelFormat = CanvasRenderer.pixelFormat
        sampleCount = CanvasRenderer.sampleCount
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
        syncViewport()
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
        syncViewport()
    }

    func send(_ event: ScrollEvent) {
        editor?.scroll(event: event)
        syncViewport()
    }

    func send(_ event: MagnifyEvent) {
        editor?.magnify(event: event)
        syncViewport()
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
    let geometry: Geometry
    let controller: CanvasController

    @MainActor private func makeCanvas() -> CanvasMetalView {
        let canvas = CanvasMetalView()
        update(canvas)
        return canvas
    }

    @MainActor private func update(_ canvas: CanvasMetalView) {
        canvas.editor = editor
        canvas.renderer?.setGeometry(geometry)
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

/// Lets chrome outside the Canvas (toolbar, buttons) command it. Commands
/// reach the core's `Editor` and the Canvas shows the result.
@MainActor
final class CanvasController {
    weak var canvas: CanvasMetalView?

    func zoomIn() { canvas?.perform { $0.zoomIn() } }
    func zoomOut() { canvas?.perform { $0.zoomOut() } }
    func resetZoom() { canvas?.perform { $0.resetZoom() } }
    func zoomToFit() { canvas?.perform { $0.zoomToFit() } }
    func setTool(_ tool: Tool) { canvas?.perform { $0.setTool(tool: tool) } }
}
