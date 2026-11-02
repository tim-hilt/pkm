import Metal
import Testing

@testable import WorkingTitle

/// Renders offscreen and probes pixels: the tracer rectangle must really be
/// visible where the core put it, at any zoom.
@MainActor @Suite struct CanvasRendererTests {
    /// A frame as 8-bit BGRA pixels, with the target the renderer drew into.
    private struct Frame {
        let width: Int
        let height: Int
        let bgra: [UInt8]

        /// Red channel at device pixel `(x, y)`; the rectangle is grey, so
        /// one channel is enough to tell ink (dark) from paper (255).
        func brightness(_ x: Int, _ y: Int) -> UInt8 { bgra[(y * width + x) * 4 + 2] }
    }

    private func render(
        viewport: Viewport, pixelsPerPoint: Float = 1, size: Int = 400
    ) throws -> (Frame, CanvasRenderer) {
        let device = try #require(MTLCreateSystemDefaultDevice())
        let renderer = try #require(CanvasRenderer(device: device))
        renderer.setGeometry(Editor().geometry())
        renderer.viewport = viewport

        func texture(samples: Int, usage: MTLTextureUsage, storage: MTLStorageMode) -> any MTLTexture {
            let descriptor = MTLTextureDescriptor()
            descriptor.pixelFormat = CanvasRenderer.pixelFormat
            descriptor.width = size
            descriptor.height = size
            descriptor.textureType = samples > 1 ? .type2DMultisample : .type2D
            descriptor.sampleCount = samples
            descriptor.usage = usage
            descriptor.storageMode = storage
            return device.makeTexture(descriptor: descriptor)!
        }
        let multisampled = texture(samples: CanvasRenderer.sampleCount, usage: .renderTarget, storage: .private)
        let resolved = texture(samples: 1, usage: [.renderTarget, .shaderRead], storage: .shared)
        let pass = MTLRenderPassDescriptor()
        pass.colorAttachments[0].texture = multisampled
        pass.colorAttachments[0].resolveTexture = resolved
        pass.colorAttachments[0].storeAction = .multisampleResolve

        renderer.renderFrame(into: pass, pixelsPerPoint: pixelsPerPoint, presenting: nil)

        // Reading back waits for the GPU: a blit/command round trip on the same queue.
        let queue = device.makeCommandQueue()!
        let sync = queue.makeCommandBuffer()!
        sync.commit()
        sync.waitUntilCompleted()
        var bgra = [UInt8](repeating: 0, count: size * size * 4)
        resolved.getBytes(&bgra, bytesPerRow: size * 4, from: MTLRegionMake2D(0, 0, size, size), mipmapLevel: 0)
        return (Frame(width: size, height: size, bgra: bgra), renderer)
    }

    @Test func rectangleOutlineIsInkAndEverythingElseIsPaper() throws {
        let (frame, _) = try render(viewport: Viewport())

        // Rectangle (50,100)-(330,280), 2 units wide: ink on the edges...
        #expect(frame.brightness(200, 100) < 60, "top edge")
        #expect(frame.brightness(50, 200) < 60, "left edge")
        #expect(frame.brightness(330, 200) < 60, "right edge")
        #expect(frame.brightness(200, 280) < 60, "bottom edge")
        // ...paper inside, outside and where the rounded corner cuts it off.
        #expect(frame.brightness(200, 190) == 255, "inside")
        #expect(frame.brightness(20, 20) == 255, "outside")
        #expect(frame.brightness(52, 102) == 255, "corner is rounded, not square")
    }

    @Test func edgesAreAntiAliased() throws {
        // The top edge spans y = 99...101. Shifting the view by half a unit makes it
        // span 98.5...100.5, so pixel row 98 is half covered: neither paper nor ink.
        let (frame, _) = try render(viewport: Viewport(origin: [0, 0.5], zoom: 1))
        #expect((100...190).contains(frame.brightness(200, 98)))
    }

    @Test func zoomAndOriginMoveTheRectangle() throws {
        // Scene point (50, 100) at the view's origin, 2x: the left edge sits at x = 0
        // and the top edge at y = 0; the right edge at (330 - 50) * 2 = 560 is off-screen.
        let (frame, _) = try render(viewport: Viewport(origin: [40, 90], zoom: 2), size: 400)
        #expect(frame.brightness(20, 200) < 60, "left edge at (50 - 40) * 2")
        #expect(frame.brightness(200, 20) < 60, "top edge at (100 - 90) * 2")
    }

    @Test func retinaScaleGrowsTheRectangleInDevicePixels() throws {
        let (frame, _) = try render(viewport: Viewport(), pixelsPerPoint: 2, size: 800)
        #expect(frame.brightness(400, 200) < 60, "top edge at 100 points = 200 pixels")
        #expect(frame.brightness(100, 400) < 60, "left edge at 50 points = 100 pixels")
    }

    @Test func eachRenderedFrameIsCounted() throws {
        let (_, renderer) = try render(viewport: Viewport())
        #expect(renderer.framesRendered == 1)
    }

    @Test func changesRequestFramesButNeverProduceThem() throws {
        let device = try #require(MTLCreateSystemDefaultDevice())
        let renderer = try #require(CanvasRenderer(device: device))
        var requests = 0
        renderer.onNeedsDisplay = { requests += 1 }

        renderer.setGeometry(Editor().geometry())
        renderer.viewport = Viewport(origin: [10, 0], zoom: 1)
        renderer.viewport = Viewport(origin: [10, 0], zoom: 1)  // unchanged

        #expect(requests == 2)
        #expect(renderer.framesRendered == 0)
    }
}
