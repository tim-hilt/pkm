import Foundation
import Testing

@testable import WorkingTitle

#if os(macOS)
    import AppKit
#else
    import UIKit
#endif

/// "No continuous render loop": a Canvas that is on screen and idle produces
/// no frames; only a requested redraw produces exactly one. And it keeps no
/// render targets in memory beyond the drawable.
@MainActor @Suite struct CanvasViewTests {
    /// Lets the main run loop (and with it any display link) run for `seconds`.
    private func idle(for seconds: TimeInterval) async {
        try? await Task.sleep(for: .seconds(seconds))
    }

    private func showCanvas() throws -> (CanvasMetalView, CanvasRenderer, AnyObject) {
        let canvas = CanvasMetalView()
        let renderer = try #require(canvas.renderer)
        renderer.setGeometry(TestScenes.rectangle())
        let frame = CGRect(x: 0, y: 0, width: 300, height: 300)
        canvas.frame = frame
        #if os(macOS)
            let window = NSWindow(contentRect: frame, styleMask: [.borderless], backing: .buffered, defer: false)
            window.contentView?.addSubview(canvas)
            window.orderFrontRegardless()
        #else
            // Only windows of a scene get displayed; use the host app's.
            let scene = try #require(UIApplication.shared.connectedScenes.first as? UIWindowScene)
            let window = try #require(scene.windows.first)
            window.addSubview(canvas)
        #endif
        return (canvas, renderer, window)
    }

    @Test func idleCanvasDrawsNothingAndRedrawRequestsDrawOnce() async throws {
        let (canvas, renderer, window) = try showCanvas()
        defer { canvas.removeFromSuperview() }
        _ = window

        await idle(for: 0.5)
        let settled = renderer.framesRendered
        #expect(settled >= 1, "the first frame is drawn when the view appears")

        await idle(for: 1)
        #expect(renderer.framesRendered == settled, "idle: no frames")

        renderer.viewport = Viewport(origin: [20, 0], zoom: 1)
        renderer.viewport = Viewport(origin: [40, 0], zoom: 1)
        await idle(for: 0.5)
        #expect(renderer.framesRendered > settled, "a change draws")
        let afterChange = renderer.framesRendered

        await idle(for: 1)
        #expect(renderer.framesRendered == afterChange, "and then stops again")
    }

    /// The 4x multisampled colour and the stencil of a full-screen Canvas
    /// would take ~110 MB on the iPad Pro; they must live in tile memory only.
    @Test func onScreenCanvasKeepsNoMultisampleOrStencilTexture() async throws {
        let (canvas, renderer, window) = try showCanvas()
        defer { canvas.removeFromSuperview() }
        _ = window

        await idle(for: 0.5)
        #expect(renderer.framesRendered >= 1)
        #expect(canvas.multisampleColorTexture == nil)
        #expect(canvas.depthStencilTexture == nil)
    }
}
