import Metal

@testable import WorkingTitle

/// Renders geometry the way the Canvas does, into memory instead of a view.
@MainActor
enum OffscreenRender {
    struct Unavailable: Error {}

    /// A production renderer on the system's GPU.
    static func makeRenderer() throws -> CanvasRenderer {
        guard let device = MTLCreateSystemDefaultDevice(), let renderer = CanvasRenderer(device: device)
        else { throw Unavailable() }
        return renderer
    }

    /// Draws `geometry` seen through `viewport` into a `size` x `size` pixel
    /// image, with the production renderer and its 4x MSAA. `pixelsPerPoint`
    /// is the screen scale being simulated.
    ///
    /// - Returns: the finished image and the renderer that drew it, so tests
    ///   can inspect renderer state such as the frame counter.
    static func render(
        _ geometry: Geometry, viewport: Viewport, pixelsPerPoint: Float = 1, size: Int = 400
    ) throws -> (image: RenderedImage, renderer: CanvasRenderer) {
        let renderer = try makeRenderer()
        renderer.setGeometry(geometry)
        renderer.viewport = viewport
        return (draw(with: renderer, pixelsPerPoint: pixelsPerPoint, size: size), renderer)
    }

    /// Renders one frame of whatever `renderer` has been given.
    static func draw(with renderer: CanvasRenderer, pixelsPerPoint: Float = 1, size: Int = 400)
        -> RenderedImage
    {
        let device = renderer.device
        func texture(
            _ format: MTLPixelFormat, samples: Int, usage: MTLTextureUsage, storage: MTLStorageMode
        ) -> any MTLTexture {
            let descriptor = MTLTextureDescriptor()
            descriptor.pixelFormat = format
            descriptor.width = size
            descriptor.height = size
            descriptor.textureType = samples > 1 ? .type2DMultisample : .type2D
            descriptor.sampleCount = samples
            descriptor.usage = usage
            descriptor.storageMode = storage
            return device.makeTexture(descriptor: descriptor)!
        }
        let format = CanvasRenderer.pixelFormat
        let multisampled = texture(
            format, samples: CanvasRenderer.sampleCount, usage: .renderTarget, storage: .private)
        let resolved = texture(format, samples: 1, usage: [.renderTarget, .shaderRead], storage: .shared)
        let stencil = texture(
            CanvasRenderer.stencilFormat, samples: CanvasRenderer.sampleCount, usage: .renderTarget,
            storage: .private)
        let pass = MTLRenderPassDescriptor()
        pass.colorAttachments[0].texture = multisampled
        pass.colorAttachments[0].resolveTexture = resolved
        pass.colorAttachments[0].storeAction = .multisampleResolve
        pass.stencilAttachment.texture = stencil

        let commands = renderer.renderFrame(into: pass, pixelsPerPoint: pixelsPerPoint, presenting: nil)
        commands?.waitUntilCompleted()

        var bgra = [UInt8](repeating: 0, count: size * size * RenderedImage.bytesPerPixel)
        resolved.getBytes(
            &bgra, bytesPerRow: size * RenderedImage.bytesPerPixel, from: MTLRegionMake2D(0, 0, size, size),
            mipmapLevel: 0)
        return RenderedImage(width: size, height: size, bgra: bgra)
    }
}
