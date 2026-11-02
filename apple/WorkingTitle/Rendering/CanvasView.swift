import MetalKit
import SwiftUI

/// The platform's Metal-backed view for the Canvas: all drawing lives in
/// `CanvasRenderer`; this class only configures `MTKView` for on-demand
/// rendering and bridges the one call that differs between UIKit and AppKit.
@MainActor
final class CanvasMetalView: MTKView {
    let renderer: CanvasRenderer?

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
    }

    @available(*, unavailable)
    required init(coder: NSCoder) { fatalError("CanvasMetalView is created in code") }

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
    let geometry: Geometry

    @MainActor private func makeCanvas() -> CanvasMetalView {
        let canvas = CanvasMetalView()
        update(canvas)
        return canvas
    }

    @MainActor private func update(_ canvas: CanvasMetalView) {
        canvas.renderer?.setGeometry(geometry)
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
