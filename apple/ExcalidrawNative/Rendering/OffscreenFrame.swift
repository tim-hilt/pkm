import Metal

extension CanvasRenderer {
    /// Renders one frame of whatever the renderer has been given into memory
    /// instead of a view, with the same pipeline and 4x MSAA as the Canvas,
    /// and waits for it. Thumbnails and tests use it.
    ///
    /// - Parameter pixelsPerPoint: Device pixels per view point; the viewport's
    ///   zoom is in points.
    /// - Returns: The image, `width` x `height` device pixels; nil if the GPU
    ///   could not provide the render target.
    func renderOffscreen(width: Int, height: Int, pixelsPerPoint: Float = 1) -> RenderedImage? {
        let descriptor = MTLTextureDescriptor.texture2DDescriptor(
            pixelFormat: Self.pixelFormat, width: width, height: height, mipmapped: false)
        descriptor.usage = .renderTarget
        descriptor.storageMode = .shared
        guard let target = device.makeTexture(descriptor: descriptor),
            let frame = renderFrame(into: target, pixelsPerPoint: pixelsPerPoint, presenting: nil)
        else { return nil }
        frame.waitUntilCompleted()

        var bgra = [UInt8](repeating: 0, count: width * height * RenderedImage.bytesPerPixel)
        target.getBytes(
            &bgra, bytesPerRow: width * RenderedImage.bytesPerPixel, from: MTLRegionMake2D(0, 0, width, height),
            mipmapLevel: 0)
        return RenderedImage(width: width, height: height, bgra: bgra)
    }
}
