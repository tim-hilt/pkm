import Metal
import MetalKit

/// Where the Canvas looks at the Scene.
struct Viewport: Equatable {
    /// Scene point shown at the top-left corner of the view.
    var origin = SIMD2<Float>.zero
    /// View points per Scene unit.
    var zoom: Float = 1
}

/// Draws the core's geometry with Metal (ADR-0002, ADR-0008). Owns the whole
/// pipeline: curve flattening, GPU resources and the draw calls; it knows
/// nothing about UIKit or AppKit (see `CanvasMetalView`).
///
/// Rendering is strictly on demand: a frame is produced only when the owner
/// asks for one, and a change of geometry or viewport only asks the owner to
/// ask (`onNeedsDisplay`); there is no render loop.
///
/// Strokes are one instanced draw per part: every segment becomes a quad that
/// the vertex shader expands to a capsule (round caps and joins) and the
/// fragment shader anti-aliases analytically; 4x MSAA smooths what is left.
///
/// A translucent stroke (hand-drawn shapes overlap themselves a lot) must not
/// blend twice where capsules overlap. It is drawn in three passes with the
/// stencil buffer: first everything that is fully covered, marking those
/// pixels so that no capsule paints over another, then the anti-aliased
/// fringe on pixels not marked, then once more without colour to clear the
/// marks. Only fringe pixels where capsules overlap still blend twice, which
/// is invisible.
///
/// Fills are stencil-then-cover: the first pass rasterises a triangle fan per
/// subpath into the stencil buffer, which counts how often the path winds
/// around each sample (or toggles it, for the even-odd rule); the second paints
/// a rectangle over the fill wherever the count is not zero and resets the
/// stencil. That is exact for concave and self-intersecting outlines and,
/// per sample, anti-aliased by the 4x MSAA.
///
/// Every draw leaves the stencil buffer zero where it touched it, so no
/// stroke or fill sees what an earlier one did there.
///
/// The multisampled colour and the stencil exist only in the GPU's tile
/// memory (`memoryless`): each frame is resolved into its single-sampled
/// target at the end of the pass, and nothing else reads them. Kept in memory
/// they would cost 20 bytes per device pixel, ~110 MB on a full-screen iPad Pro
/// and far more for a large PNG export.
///
/// Text is filled too: each line of text becomes the outlines of its glyphs
/// (cached per glyph, see `Excalifont`) in one fill, so it stays sharp at any
/// zoom.
///
/// Images are textured quads (see `ImageTextures`): the core describes an
/// image by file id, box, crop and scale, the renderer decodes the file with
/// ImageIO once per file id and samples the texture, mirrored, turned and
/// clipped to rounded corners in the shader. Without pixels for the file
/// (missing, not an image) an Excalidraw-style placeholder is filled instead.
///
/// What is drawn for an Element is cached per Element handle and version (and
/// flattening level), so a changed Scene only rebuilds the Elements that changed.
///
/// The overlay (selection frames) is drawn over the Scene in the same frame.
/// It is a few strokes made for one zoom, so it is rebuilt whenever it or the
/// flattening level changes, and not cached per Element.
@MainActor
final class CanvasRenderer: NSObject, MTKViewDelegate {
    /// Samples per pixel of the colour and stencil a frame is drawn into.
    private static let sampleCount = 4
    /// The format of the textures frames are rendered into.
    static let pixelFormat = MTLPixelFormat.bgra8Unorm
    private static let stencilFormat = MTLPixelFormat.stencil8

    /// Largest tolerated flattening error, in device pixels.
    private static let flatteningTolerancePixels: Float = 0.1

    /// Called when a new frame is needed. The owner decides when to draw it.
    var onNeedsDisplay: (() -> Void)?

    var viewport = Viewport() {
        didSet { if viewport != oldValue { onNeedsDisplay?() } }
    }

    /// Frames rendered so far. Stays constant while nothing changes, which is
    /// how "no continuous render loop" is verified.
    private(set) var framesRendered = 0
    /// The colour of the paper as `0xRRGGBBAA`: the Canvas colour the core
    /// reports (`Editor.canvasColor`); white by default.
    var paperColor: UInt32 = 0xffff_ffff {
        didSet { if paperColor != oldValue { onNeedsDisplay?() } }
    }

    /// Whether the Canvas is dark: image placeholders have their dark colours
    /// then. The Scene's own colours come from the core already transformed.
    var isDark = false {
        didSet {
            guard isDark != oldValue else { return }
            cache.removeAll()
            drawsAreStale = true
            onNeedsDisplay?()
        }
    }

    private let queue: any MTLCommandQueue
    private let pipeline: any MTLRenderPipelineState
    /// The fill shaders: into the stencil buffer only, and onto the colour target.
    private let fillStencilPipeline: any MTLRenderPipelineState
    private let fillCoverPipeline: any MTLRenderPipelineState
    private let imagePipeline: any MTLRenderPipelineState
    private let imageSampler: any MTLSamplerState
    let device: any MTLDevice

    /// Where image files come from. Set it before the geometry is drawn; the
    /// `CanvasMetalView` does so with its editor. File ids mean nothing
    /// outside their source, so another source starts with no textures.
    var files: (any ImageFileSource)? {
        didSet {
            guard files !== oldValue else { return }
            textures.keepOnly([])
            // Cached draws hold the old source's textures and placeholders.
            cache.removeAll()
            drawsAreStale = true
            onNeedsDisplay?()
        }
    }

    /// The decoded image files, one texture each.
    let textures: ImageTextures

    /// Depth-stencil states for opaque strokes, and for the first two passes
    /// of translucent ones (the third is `clearStencil`).
    private let plain: any MTLDepthStencilState
    private let interior: any MTLDepthStencilState
    private let fringe: any MTLDepthStencilState
    /// Depth-stencil states of fills: counting the winding by either rule and
    /// painting where it is not zero (and resetting it there). `clearStencil`
    /// zeroes whatever it covers.
    private let windingNonZero: any MTLDepthStencilState
    private let windingEvenOdd: any MTLDepthStencilState
    private let paintInside: any MTLDepthStencilState
    private let clearStencil: any MTLDepthStencilState

    private(set) var geometry: (any GeometrySource)?
    /// What is drawn over the Scene, see `setOverlay`.
    private(set) var overlay: (any GeometrySource)?
    /// Everything to draw, in z-order: the cached draws of all Elements.
    private var draws: [Draw] = []
    /// What is drawn over the overlay, see `setDimensions`.
    private(set) var dimensions: (any GeometrySource)?
    /// How strongly `dimensions` are drawn, 1 for fully down to 0.
    var dimensionsStrength: Float = 1 {
        didSet {
            guard dimensionsStrength != oldValue else { return }
            overlayIsStale = true
            onNeedsDisplay?()
        }
    }
    /// The draws of `overlay` and `dimensions` (at `dimensionsStrength`), and the flattening level they were built for.
    private var overlayDraws: [Draw] = []
    private var overlayLevel: Int?
    private var overlayIsStale = true
    private var cache: [UInt32: CachedElement] = [:]
    /// The `Geometry.generation` the cache belongs to.
    private var cacheGeneration: UInt64?
    /// Power-of-two zoom level `draws` were flattened for; curves are only
    /// re-flattened when it changes, not on every zoom step.
    private var flatteningLevel: Int?
    private var drawsAreStale = true
    /// Elements whose draws were built (flattened and uploaded) so far; a
    /// cache hit does not count. Tests use it to see what was rebuilt.
    private(set) var elementsBuilt = 0
    /// Draws the lines of text; nil if the font is missing, which leaves text out.
    private let font = Excalifont.shared
    /// The memoryless multisampled colour and stencil of the last frame,
    /// reused while the frame size stays.
    private var frameTargets: (color: any MTLTexture, stencil: any MTLTexture)?

    /// A stroked part's segments on the GPU plus how to draw them.
    private struct StrokeDraw {
        var segments: any MTLBuffer
        var segmentCount: Int
        var uniforms: PartUniforms
        /// Whether the stroke needs the two-pass treatment.
        var isTranslucent: Bool
    }

    /// A filled part's triangles on the GPU plus how to draw them.
    private struct FillDraw {
        /// The fan triangles, then the cover rectangle's two triangles.
        var vertices: any MTLBuffer
        var fanVertexCount: Int
        var uniforms: PartUniforms
        var rule: GeometryFillRule
    }

    /// An image: the texture of its file and where and how to put it.
    private struct ImageDraw {
        var texture: any MTLTexture
        var uniforms: ImageUniforms
    }

    private enum Draw {
        case stroke(StrokeDraw)
        case fill(FillDraw)
        case image(ImageDraw)

        /// This draw at `strength` of its own opacity.
        func faded(by strength: Float) -> Draw {
            switch self {
            case .stroke(var stroke):
                // The colour is premultiplied: scaling all channels scales the opacity.
                stroke.uniforms.color *= strength
                return .stroke(stroke)
            case .fill(var fill):
                fill.uniforms.color *= strength
                return .fill(fill)
            case .image(var image):
                image.uniforms.opacity *= strength
                return .image(image)
            }
        }
    }

    /// The draws of one Element at the version and flattening level they were built for.
    private struct CachedElement {
        var version: UInt32
        var level: Int
        var draws: [Draw]
    }

    /// Mirrors `FrameUniforms` in StrokeShaders.metal.
    private struct FrameUniforms {
        var origin: SIMD2<Float>
        var viewSize: SIMD2<Float>
        var scale: Float
    }

    /// Mirrors `PartUniforms` in StrokeShaders.metal.
    private struct PartUniforms {
        var color: SIMD4<Float>
        var width: Float
        var interiorOnly: Float = 0
        /// See `GeometryPart.hole`.
        var hole: SIMD4<Float> = .zero
    }

    /// Mirrors `ImageUniforms` in StrokeShaders.metal.
    private struct ImageUniforms {
        var box: SIMD4<Float>
        var uv: SIMD4<Float>
        var scale: SIMD2<Float>
        var angle: Float
        var opacity: Float
        var cornerRadius: Float

        init(_ image: GeometryImage) {
            box = SIMD4(image.x, image.y, image.width, image.height)
            uv = SIMD4(image.u0, image.v0, image.u1, image.v1)
            scale = SIMD2(image.scaleX, image.scaleY)
            angle = image.angle
            opacity = image.opacity
            cornerRadius = image.cornerRadius
        }
    }

    init?(device: any MTLDevice) {
        guard
            let queue = device.makeCommandQueue(),
            let library = try? device.makeDefaultLibrary(bundle: Bundle(for: CanvasRenderer.self)),
            let vertex = library.makeFunction(name: "stroke_vertex"),
            let fragment = library.makeFunction(name: "stroke_fragment"),
            let imageVertex = library.makeFunction(name: "image_vertex"),
            let imageFragment = library.makeFunction(name: "image_fragment")
        else { return nil }

        let descriptor = MTLRenderPipelineDescriptor()
        descriptor.vertexFunction = vertex
        descriptor.fragmentFunction = fragment
        descriptor.rasterSampleCount = Self.sampleCount
        let color = descriptor.colorAttachments[0]!
        descriptor.stencilAttachmentPixelFormat = Self.stencilFormat
        color.pixelFormat = Self.pixelFormat
        // Premultiplied source-over.
        color.isBlendingEnabled = true
        color.sourceRGBBlendFactor = .one
        color.sourceAlphaBlendFactor = .one
        color.destinationRGBBlendFactor = .oneMinusSourceAlpha
        color.destinationAlphaBlendFactor = .oneMinusSourceAlpha
        guard let pipeline = try? device.makeRenderPipelineState(descriptor: descriptor) else { return nil }

        // Fills: the same target, other shaders; the first pipeline only
        // feeds the stencil buffer.
        guard let fillVertex = library.makeFunction(name: "fill_vertex"),
            let fillFragment = library.makeFunction(name: "fill_fragment")
        else { return nil }
        descriptor.vertexFunction = fillVertex
        descriptor.fragmentFunction = fillFragment
        guard let fillCoverPipeline = try? device.makeRenderPipelineState(descriptor: descriptor) else { return nil }
        descriptor.vertexFunction = imageVertex
        descriptor.fragmentFunction = imageFragment
        guard let imagePipeline = try? device.makeRenderPipelineState(descriptor: descriptor) else { return nil }
        // Pictures are smoothed when scaled and, with their mipmaps, when
        // shrunk; nothing outside them is ever sampled.
        let sampling = MTLSamplerDescriptor()
        sampling.minFilter = .linear
        sampling.magFilter = .linear
        sampling.mipFilter = .linear
        sampling.sAddressMode = .clampToEdge
        sampling.tAddressMode = .clampToEdge
        guard let imageSampler = device.makeSamplerState(descriptor: sampling) else { return nil }
        descriptor.vertexFunction = fillVertex
        descriptor.fragmentFunction = fillFragment
        color.writeMask = []
        guard let fillStencilPipeline = try? device.makeRenderPipelineState(descriptor: descriptor) else {
            return nil
        }

        /// A state whose front- and back-facing triangles pass the stencil
        /// test as `compare` says and then do `pass` to the stencil value.
        func stencilState(
            compare: MTLCompareFunction, pass: MTLStencilOperation, backPass: MTLStencilOperation? = nil
        ) -> (any MTLDepthStencilState)? {
            func stencil(_ operation: MTLStencilOperation) -> MTLStencilDescriptor {
                let stencil = MTLStencilDescriptor()
                stencil.stencilCompareFunction = compare
                stencil.depthStencilPassOperation = operation
                return stencil
            }
            let state = MTLDepthStencilDescriptor()
            state.frontFaceStencil = stencil(pass)
            state.backFaceStencil = stencil(backPass ?? pass)
            return device.makeDepthStencilState(descriptor: state)
        }
        guard
            let plain = stencilState(compare: .always, pass: .keep),
            let interior = stencilState(compare: .notEqual, pass: .replace),
            let fringe = stencilState(compare: .notEqual, pass: .keep),
            // Triangles facing one way count up and the others down, so that
            // the stencil value is the winding number (mod 256).
            let windingNonZero = stencilState(compare: .always, pass: .incrementWrap, backPass: .decrementWrap),
            let windingEvenOdd = stencilState(compare: .always, pass: .invert),
            let paintInside = stencilState(compare: .notEqual, pass: .zero),
            let clearStencil = stencilState(compare: .always, pass: .zero)
        else { return nil }

        self.device = device
        self.queue = queue
        self.pipeline = pipeline
        self.fillStencilPipeline = fillStencilPipeline
        self.fillCoverPipeline = fillCoverPipeline
        self.imagePipeline = imagePipeline
        self.imageSampler = imageSampler
        self.textures = ImageTextures(device: device, queue: queue)
        self.plain = plain
        self.interior = interior
        self.fringe = fringe
        self.windingNonZero = windingNonZero
        self.windingEvenOdd = windingEvenOdd
        self.paintInside = paintInside
        self.clearStencil = clearStencil
    }

    /// Replaces what is drawn; the same object again is a no-op. Keeps
    /// `geometry` alive: curves are flattened from the core's buffers again
    /// whenever the zoom level changes. Elements whose version is unchanged
    /// are not rebuilt.
    func setGeometry(_ geometry: any GeometrySource) {
        guard geometry !== self.geometry else { return }
        self.geometry = geometry
        drawsAreStale = true
        onNeedsDisplay?()
    }

    /// Replaces what is drawn over the Scene (selection frames, in Scene
    /// coordinates like the geometry); nil draws nothing. The same object
    /// again is a no-op.
    func setOverlay(_ overlay: (any GeometrySource)?) {
        guard overlay !== self.overlay else { return }
        self.overlay = overlay
        overlayIsStale = true
        onNeedsDisplay?()
    }

    /// Replaces what is drawn over the overlay (Dimensions, in Scene
    /// coordinates like the overlay); nil draws nothing. The same object again
    /// is a no-op.
    func setDimensions(_ dimensions: (any GeometrySource)?) {
        guard dimensions !== self.dimensions else { return }
        self.dimensions = dimensions
        overlayIsStale = true
        onNeedsDisplay?()
    }

    /// `rgba` (`0xRRGGBBAA`) as the colour to clear to.
    private static func clearColor(_ rgba: UInt32) -> MTLClearColor {
        let channel = { (shift: UInt32) in Double((rgba >> shift) & 0xff) / 255 }
        return MTLClearColor(red: channel(24), green: channel(16), blue: channel(8), alpha: channel(0))
    }

    /// Renders one frame into `target` (a single-sampled 2D texture in
    /// `pixelFormat` with `.renderTarget` usage, such as a drawable's) and
    /// presents `drawable` if given. The target is cleared to the Canvas
    /// background first.
    ///
    /// - Parameter pixelsPerPoint: Device pixels per view point (the screen scale).
    /// - Returns: The submitted work, for callers that read the target back and
    ///   so must wait for it; nil if the GPU could not take it.
    @discardableResult
    func renderFrame(into target: any MTLTexture, pixelsPerPoint: Float, presenting drawable: (any MTLDrawable)?)
        -> (any MTLCommandBuffer)?
    {
        guard let (multisampled, stencil) = frameTargets(width: target.width, height: target.height),
            let commands = queue.makeCommandBuffer()
        else { return nil }
        let pass = MTLRenderPassDescriptor()
        pass.colorAttachments[0].texture = multisampled
        pass.colorAttachments[0].resolveTexture = target
        pass.colorAttachments[0].loadAction = .clear
        pass.colorAttachments[0].storeAction = .multisampleResolve
        pass.colorAttachments[0].clearColor = Self.clearColor(paperColor)
        pass.stencilAttachment.texture = stencil
        pass.stencilAttachment.loadAction = .clear
        pass.stencilAttachment.clearStencil = 0
        pass.stencilAttachment.storeAction = .dontCare

        let scale = viewport.zoom * pixelsPerPoint
        let level = Self.flatteningLevel(forScale: scale)
        refreshDraws(level: level)
        refreshOverlay(level: level)

        if let encoder = commands.makeRenderCommandEncoder(descriptor: pass) {
            var frame = FrameUniforms(
                origin: viewport.origin, viewSize: SIMD2(Float(target.width), Float(target.height)), scale: scale)
            encoder.setVertexBytes(&frame, length: MemoryLayout<FrameUniforms>.stride, index: 1)
            encoder.setFragmentBytes(&frame, length: MemoryLayout<FrameUniforms>.stride, index: 1)
            var drawingStrokes = false
            // The Scene, then the overlay on top; no copy of the (large) draws.
            for items in [draws, overlayDraws] {
                for item in items {
                    switch item {
                    case .stroke(let stroke):
                        if !drawingStrokes {
                            encoder.setRenderPipelineState(pipeline)
                            drawingStrokes = true
                        }
                        encoder.setVertexBuffer(stroke.segments, offset: 0, index: 0)
                        if stroke.isTranslucent {
                            encoder.setStencilReferenceValue(1)
                            var uniforms = stroke.uniforms
                            uniforms.interiorOnly = 1
                            draw(stroke, uniforms, with: interior, in: encoder)
                            uniforms.interiorOnly = 0
                            draw(stroke, uniforms, with: fringe, in: encoder)
                            // No colour: premultiplied zero blends to what is there.
                            uniforms.color = .zero
                            draw(stroke, uniforms, with: clearStencil, in: encoder)
                        } else {
                            draw(stroke, stroke.uniforms, with: plain, in: encoder)
                        }
                    case .fill(let fill):
                        drawingStrokes = false
                        draw(fill, in: encoder)
                    case .image(let image):
                        drawingStrokes = false
                        draw(image, in: encoder)
                    }
                }
            }
            encoder.endEncoding()
        }
        if let drawable { commands.present(drawable) }
        commands.commit()
        framesRendered += 1
        return commands
    }

    private func draw(
        _ stroke: StrokeDraw, _ uniforms: PartUniforms, with state: any MTLDepthStencilState,
        in encoder: any MTLRenderCommandEncoder
    ) {
        var uniforms = uniforms
        encoder.setDepthStencilState(state)
        encoder.setVertexBytes(&uniforms, length: MemoryLayout<PartUniforms>.stride, index: 2)
        encoder.setFragmentBytes(&uniforms, length: MemoryLayout<PartUniforms>.stride, index: 2)
        encoder.drawPrimitives(
            type: .triangleStrip, vertexStart: 0, vertexCount: 4, instanceCount: stroke.segmentCount)
    }

    private func draw(_ image: ImageDraw, in encoder: any MTLRenderCommandEncoder) {
        var uniforms = image.uniforms
        encoder.setRenderPipelineState(imagePipeline)
        encoder.setDepthStencilState(plain)
        encoder.setVertexBytes(&uniforms, length: MemoryLayout<ImageUniforms>.stride, index: 2)
        encoder.setFragmentBytes(&uniforms, length: MemoryLayout<ImageUniforms>.stride, index: 2)
        encoder.setFragmentTexture(image.texture, index: 0)
        encoder.setFragmentSamplerState(imageSampler, index: 0)
        encoder.drawPrimitives(type: .triangleStrip, vertexStart: 0, vertexCount: 4)
    }

    /// Draws `fill` with stencil-then-cover. The stencil is zero again where it
    /// ends, so fills never need the whole buffer cleared.
    private func draw(_ fill: FillDraw, in encoder: any MTLRenderCommandEncoder) {
        var uniforms = fill.uniforms
        let coverStart = fill.fanVertexCount
        encoder.setVertexBuffer(fill.vertices, offset: 0, index: 0)
        // Both passes leave out the hole, so the stencil stays zero there.
        encoder.setFragmentBytes(&uniforms, length: MemoryLayout<PartUniforms>.stride, index: 2)
        encoder.setRenderPipelineState(fillStencilPipeline)
        encoder.setStencilReferenceValue(0)
        encoder.setDepthStencilState(fill.rule == .evenOdd ? windingEvenOdd : windingNonZero)
        encoder.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: coverStart)

        encoder.setRenderPipelineState(fillCoverPipeline)
        encoder.setDepthStencilState(paintInside)
        encoder.drawPrimitives(type: .triangle, vertexStart: coverStart, vertexCount: 6)
    }

    // MARK: MTKViewDelegate

    func mtkView(_ view: MTKView, drawableSizeWillChange size: CGSize) {
        onNeedsDisplay?()
    }

    func draw(in view: MTKView) {
        guard view.bounds.width > 0, let drawable = view.currentDrawable else { return }
        let pixelsPerPoint = Float(view.drawableSize.width / view.bounds.width)
        renderFrame(into: drawable.texture, pixelsPerPoint: pixelsPerPoint, presenting: drawable)
    }

    /// The multisampled colour and stencil for a frame of `width` x `height`
    /// pixels; memoryless (see the class comment), so a new size costs only
    /// two texture objects.
    private func frameTargets(width: Int, height: Int) -> (any MTLTexture, any MTLTexture)? {
        if let targets = frameTargets, targets.color.width == width, targets.color.height == height {
            return targets
        }
        func texture(_ format: MTLPixelFormat) -> (any MTLTexture)? {
            let descriptor = MTLTextureDescriptor()
            descriptor.textureType = .type2DMultisample
            descriptor.pixelFormat = format
            descriptor.width = width
            descriptor.height = height
            descriptor.sampleCount = Self.sampleCount
            descriptor.usage = .renderTarget
            descriptor.storageMode = .memoryless
            return device.makeTexture(descriptor: descriptor)
        }
        guard let color = texture(Self.pixelFormat), let stencil = texture(Self.stencilFormat) else { return nil }
        frameTargets = (color, stencil)
        return (color, stencil)
    }

    // MARK: Flattening

    /// The power of two that curves are flattened for at `scale` device
    /// pixels per Scene unit.
    private static func flatteningLevel(forScale scale: Float) -> Int {
        Int(log2(max(scale, .leastNormalMagnitude)).rounded(.up))
    }

    /// Brings `overlayDraws` up to date with the overlay and the flattening level.
    private func refreshOverlay(level: Int) {
        guard overlayIsStale || level != overlayLevel else { return }
        overlayIsStale = false
        overlayLevel = level
        let tolerance = Self.flatteningTolerancePixels / exp2(Float(level))
        func draws(of source: (any GeometrySource)?) -> [Draw] {
            source?.withBuffers { buffers in
                buffers.parts.compactMap { makeDraw(for: $0, in: buffers, tolerance: tolerance) }
            } ?? []
        }
        overlayDraws = draws(of: overlay) + draws(of: dimensions).map { $0.faded(by: dimensionsStrength) }
    }

    /// Brings `draws` up to date with the geometry and the zoom: rebuilds
    /// the Elements that changed or were flattened for another power-of-two
    /// zoom level, reuses the others.
    private func refreshDraws(level: Int) {
        guard drawsAreStale || level != flatteningLevel, let geometry else { return }
        drawsAreStale = false
        flatteningLevel = level
        if geometry.generation() != cacheGeneration {
            // A Drawing was loaded: handles mean other Elements now, and it may
            // have files that were missing.
            cache.removeAll()
            textures.forgetFailures()
            cacheGeneration = geometry.generation()
        }
        let tolerance = Self.flatteningTolerancePixels / exp2(Float(level))

        var kept: [UInt32: CachedElement] = [:]
        var all: [Draw] = []
        var referencedFiles = Set<String>()
        geometry.withBuffers { buffers in
            for item in buffers.elementsInZOrder() {
                if case .image(let index) = item.content {
                    referencedFiles.insert(buffers.fileID(of: buffers.images[index]))
                }
                var entry = cache[item.element]
                if entry?.version != item.version || entry?.level != level {
                    let draws: [Draw] =
                        switch item.content {
                        case .parts(let run):
                            run.compactMap { makeDraw(for: buffers.parts[$0], in: buffers, tolerance: tolerance) }
                        case .textLines(let run):
                            run.compactMap { makeDraw(for: buffers.textLines[$0], in: buffers, tolerance: tolerance) }
                        case .image(let index):
                            makeDraws(for: buffers.images[index], in: buffers, tolerance: tolerance)
                        }
                    entry = CachedElement(version: item.version, level: level, draws: draws)
                    elementsBuilt += 1
                }
                if let entry {
                    kept[item.element] = entry
                    all += entry.draws
                }
            }
        }
        cache = kept
        draws = all
        // Files no image shows any more give up their textures.
        textures.keepOnly(referencedFiles)
    }

    /// The draws of an image: its picture, or a placeholder while there are
    /// no pixels for its file.
    private func makeDraws(for image: GeometryImage, in buffers: GeometryBuffers, tolerance: Float) -> [Draw] {
        let id = buffers.fileID(of: image)
        if !id.isEmpty, let texture = textures.texture(forFile: id, from: files) {
            return [.image(ImageDraw(texture: texture, uniforms: ImageUniforms(image)))]
        }
        let failed = image.imageStatus == .error || textures.isUndecodable(id)
        return ImagePlaceholder.fills(width: image.width, height: image.height, failed: failed, dark: isDark).compactMap
        { fill in
            // The placeholder is made in the box's frame; the image's
            // placement takes it into the Scene.
            var coords = fill.path.coords
            for index in stride(from: 0, to: coords.count, by: 2) {
                let point = image.scenePoint(SIMD2(coords[index], coords[index + 1]))
                (coords[index], coords[index + 1]) = (point.x, point.y)
            }
            guard let mesh = FillMesh.mesh(verbs: fill.path.verbs, coords: coords, tolerance: tolerance)
            else { return nil }
            return fillDraw(mesh, color: premultipliedColor(fill.color, opacity: image.opacity), rule: .nonZero)
                .map(Draw.fill)
        }
    }

    private func makeDraw(for part: GeometryPart, in buffers: GeometryBuffers, tolerance: Float) -> Draw? {
        switch part.kind {
        case GeometryPart.strokeKind:
            return strokeDraw(for: part, in: buffers, tolerance: tolerance).map(Draw.stroke)
        case GeometryPart.fillKind:
            return fillDraw(for: part, in: buffers, tolerance: tolerance).map(Draw.fill)
        default:
            return nil
        }
    }

    private func strokeDraw(for part: GeometryPart, in buffers: GeometryBuffers, tolerance: Float) -> StrokeDraw? {
        let segments = StrokeFlattener.segments(of: part, in: buffers, tolerance: tolerance)
        guard !segments.isEmpty,
            let buffer = segments.withUnsafeBytes({
                device.makeBuffer(bytes: $0.baseAddress!, length: $0.count, options: .storageModeShared)
            })
        else { return nil }
        let color = premultipliedColor(of: part)
        return StrokeDraw(
            segments: buffer, segmentCount: segments.count,
            uniforms: PartUniforms(color: color, width: part.width, hole: part.hole), isTranslucent: color.w < 1)
    }

    private func makeDraw(for line: GeometryTextLine, in buffers: GeometryBuffers, tolerance: Float) -> Draw? {
        guard let font else { return nil }
        let path = TextLineOutline.path(of: line, text: buffers.string(of: line), font: font)
        guard let mesh = FillMesh.mesh(verbs: path.verbs, coords: path.coords, tolerance: tolerance) else {
            return nil
        }
        return fillDraw(
            mesh, color: premultipliedColor(line.color, opacity: line.opacity), rule: .nonZero
        ).map(Draw.fill)
    }

    private func fillDraw(for part: GeometryPart, in buffers: GeometryBuffers, tolerance: Float) -> FillDraw? {
        guard let rule = part.rule,
            let mesh = FillMesh.mesh(of: part, in: buffers, tolerance: tolerance)
        else { return nil }
        return fillDraw(mesh, color: premultipliedColor(of: part), rule: rule, hole: part.hole)
    }

    private func fillDraw(
        _ mesh: FillMesh, color: SIMD4<Float>, rule: GeometryFillRule, hole: SIMD4<Float> = .zero
    ) -> FillDraw? {
        let vertices = mesh.triangles + mesh.cover
        guard
            let buffer = vertices.withUnsafeBytes({
                device.makeBuffer(bytes: $0.baseAddress!, length: $0.count, options: .storageModeShared)
            })
        else { return nil }
        return FillDraw(
            vertices: buffer, fanVertexCount: mesh.triangles.count,
            uniforms: PartUniforms(color: color, width: 0, hole: hole), rule: rule)
    }

    private func premultipliedColor(of part: GeometryPart) -> SIMD4<Float> {
        premultipliedColor(part.color, opacity: part.opacity)
    }

    private func premultipliedColor(_ color: UInt32, opacity: Float) -> SIMD4<Float> {
        func channel(_ shift: UInt32) -> Float { Float((color >> shift) & 0xff) / 255 }
        let alpha = channel(0) * opacity
        return SIMD4(channel(24) * alpha, channel(16) * alpha, channel(8) * alpha, alpha)
    }
}
