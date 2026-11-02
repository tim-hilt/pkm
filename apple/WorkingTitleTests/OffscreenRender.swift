import Metal

@testable import WorkingTitle

/// Renders geometry the way the Canvas does, into memory instead of a view.
@MainActor
enum OffscreenRender {
    struct Unavailable: Error {}

    /// Draws `geometry` seen through `viewport` into a `size` x `size` pixel
    /// image, with the production renderer and its 4x MSAA. `pixelsPerPoint`
    /// is the screen scale being simulated.
    ///
    /// - Returns: the finished image and the renderer that drew it, so tests
    ///   can inspect renderer state such as the frame counter.
    static func render(
        _ geometry: Geometry, viewport: Viewport, pixelsPerPoint: Float = 1, size: Int = 400
    ) throws -> (image: RenderedImage, renderer: CanvasRenderer) {
        guard let device = MTLCreateSystemDefaultDevice(), let renderer = CanvasRenderer(device: device)
        else { throw Unavailable() }
        renderer.setGeometry(geometry)
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

        let commands = renderer.renderFrame(into: pass, pixelsPerPoint: pixelsPerPoint, presenting: nil)
        commands?.waitUntilCompleted()

        var bgra = [UInt8](repeating: 0, count: size * size * RenderedImage.bytesPerPixel)
        resolved.getBytes(
            &bgra, bytesPerRow: size * RenderedImage.bytesPerPixel, from: MTLRegionMake2D(0, 0, size, size),
            mipmapLevel: 0)
        return (RenderedImage(width: size, height: size, bgra: bgra), renderer)
    }
}
