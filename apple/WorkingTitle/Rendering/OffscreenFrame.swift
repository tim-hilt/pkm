import Metal

extension CanvasRenderer {
    /// Renders one frame of whatever the renderer has been given into memory
    /// instead of a view, with the same pipeline and 4x MSAA as the Canvas,
    /// and waits for it. Thumbnails and tests use it.
    ///
    /// - Parameter pixelsPerPoint: Device pixels per view point; the viewport's
    ///   zoom is in points.
    /// - Returns: The image, `width` x `height` device pixels; nil if the GPU
    ///   could not provide the render targets.
    func renderOffscreen(width: Int, height: Int, pixelsPerPoint: Float = 1) -> RenderedImage? {
        func texture(
            _ format: MTLPixelFormat, samples: Int, usage: MTLTextureUsage, storage: MTLStorageMode
        ) -> (any MTLTexture)? {
            let descriptor = MTLTextureDescriptor()
            descriptor.pixelFormat = format
            descriptor.width = width
            descriptor.height = height
            descriptor.textureType = samples > 1 ? .type2DMultisample : .type2D
            descriptor.sampleCount = samples
            descriptor.usage = usage
            descriptor.storageMode = storage
            return device.makeTexture(descriptor: descriptor)
        }
        guard
            let multisampled = texture(
                Self.pixelFormat, samples: Self.sampleCount, usage: .renderTarget, storage: .private),
            let resolved = texture(
                Self.pixelFormat, samples: 1, usage: [.renderTarget, .shaderRead], storage: .shared),
            let stencil = texture(
                Self.stencilFormat, samples: Self.sampleCount, usage: .renderTarget, storage: .private)
        else { return nil }
        let pass = MTLRenderPassDescriptor()
        pass.colorAttachments[0].texture = multisampled
        pass.colorAttachments[0].resolveTexture = resolved
        pass.colorAttachments[0].storeAction = .multisampleResolve
        pass.stencilAttachment.texture = stencil

        renderFrame(into: pass, pixelsPerPoint: pixelsPerPoint, presenting: nil)?.waitUntilCompleted()

        var bgra = [UInt8](repeating: 0, count: width * height * RenderedImage.bytesPerPixel)
        resolved.getBytes(
            &bgra, bytesPerRow: width * RenderedImage.bytesPerPixel, from: MTLRegionMake2D(0, 0, width, height),
            mipmapLevel: 0)
        return RenderedImage(width: width, height: height, bgra: bgra)
    }
}
