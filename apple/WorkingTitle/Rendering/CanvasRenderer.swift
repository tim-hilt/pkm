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
/// blend twice where capsules overlap. It is drawn in two passes with the
/// stencil buffer: first everything that is fully covered, marking those
/// pixels so that no capsule paints over another, then the anti-aliased
/// fringe on pixels not marked. Only fringe
/// pixels where capsules overlap still blend twice, which is invisible.
///
/// What is drawn for an Element is cached per Element handle and version (and
/// flattening level), so a changed Scene only rebuilds the Elements that changed.
@MainActor
final class CanvasRenderer: NSObject, MTKViewDelegate {
    /// Samples per pixel; the render target must be multisampled with this count.
    static let sampleCount = 4
    static let pixelFormat = MTLPixelFormat.bgra8Unorm
    /// The render target needs a stencil attachment in this format.
    static let stencilFormat = MTLPixelFormat.stencil8

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

    private let queue: any MTLCommandQueue
    private let pipeline: any MTLRenderPipelineState
    let device: any MTLDevice

    /// Depth-stencil states for opaque strokes, and for the two passes of
    /// translucent ones.
    private let plain: any MTLDepthStencilState
    private let interior: any MTLDepthStencilState
    private let fringe: any MTLDepthStencilState

    private(set) var geometry: Geometry?
    /// Everything to draw, in z-order: the cached draws of all Elements.
    private var strokes: [StrokeDraw] = []
    private var cache: [UInt32: CachedElement] = [:]
    /// The `Geometry.generation` the cache belongs to.
    private var cacheGeneration: UInt64?
    /// Power-of-two zoom level `strokes` were flattened for; curves are only
    /// re-flattened when it changes, not on every zoom step.
    private var flatteningLevel: Int?
    private var strokesAreStale = true
    /// Elements whose strokes were built (flattened and uploaded) so far; a
    /// cache hit does not count. Tests use it to see what was rebuilt.
    private(set) var elementsBuilt = 0

    /// A part's segments on the GPU plus how to draw them.
    private struct StrokeDraw {
        var segments: any MTLBuffer
        var segmentCount: Int
        var uniforms: PartUniforms
        /// Whether the stroke needs the two-pass treatment.
        var isTranslucent: Bool
    }

    /// The draws of one Element at the version and flattening level they were built for.
    private struct CachedElement {
        var version: UInt32
        var level: Int
        var draws: [StrokeDraw]
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
    }

    init?(device: any MTLDevice) {
        guard
            let queue = device.makeCommandQueue(),
            let library = try? device.makeDefaultLibrary(bundle: Bundle(for: CanvasRenderer.self)),
            let vertex = library.makeFunction(name: "stroke_vertex"),
            let fragment = library.makeFunction(name: "stroke_fragment")
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

        func stencilState(compare: MTLCompareFunction, pass: MTLStencilOperation) -> (any MTLDepthStencilState)? {
            let stencil = MTLStencilDescriptor()
            stencil.stencilCompareFunction = compare
            stencil.depthStencilPassOperation = pass
            let state = MTLDepthStencilDescriptor()
            state.frontFaceStencil = stencil
            state.backFaceStencil = stencil
            return device.makeDepthStencilState(descriptor: state)
        }
        guard
            let plain = stencilState(compare: .always, pass: .keep),
            let interior = stencilState(compare: .notEqual, pass: .replace),
            let fringe = stencilState(compare: .notEqual, pass: .keep)
        else { return nil }

        self.device = device
        self.queue = queue
        self.pipeline = pipeline
        self.plain = plain
        self.interior = interior
        self.fringe = fringe
    }

    /// Replaces what is drawn; the same object again is a no-op. Keeps
    /// `geometry` alive: curves are flattened from the core's buffers again
    /// whenever the zoom level changes. Elements whose version is unchanged
    /// are not rebuilt.
    func setGeometry(_ geometry: Geometry) {
        guard geometry !== self.geometry else { return }
        self.geometry = geometry
        strokesAreStale = true
        onNeedsDisplay?()
    }

    /// Renders one frame into `pass` (whose first colour attachment must be
    /// `sampleCount`-multisampled in `pixelFormat`) and presents `drawable`
    /// if given. The target is cleared to the Canvas background first.
    ///
    /// - Parameter pixelsPerPoint: Device pixels per view point (the screen scale).
    /// - Returns: The submitted work, for callers that read the target back and
    ///   so must wait for it; nil if there was nothing to render into.
    @discardableResult
    func renderFrame(into pass: MTLRenderPassDescriptor, pixelsPerPoint: Float, presenting drawable: (any MTLDrawable)?)
        -> (any MTLCommandBuffer)?
    {
        guard let target = pass.colorAttachments[0].texture,
            let commands = queue.makeCommandBuffer()
        else { return nil }
        pass.colorAttachments[0].loadAction = .clear
        pass.colorAttachments[0].clearColor = MTLClearColor(red: 1, green: 1, blue: 1, alpha: 1)
        pass.stencilAttachment.loadAction = .clear
        pass.stencilAttachment.clearStencil = 0
        pass.stencilAttachment.storeAction = .dontCare

        let scale = viewport.zoom * pixelsPerPoint
        refreshStrokes(scale: scale)

        if let encoder = commands.makeRenderCommandEncoder(descriptor: pass) {
            encoder.setRenderPipelineState(pipeline)
            var frame = FrameUniforms(
                origin: viewport.origin, viewSize: SIMD2(Float(target.width), Float(target.height)), scale: scale)
            encoder.setVertexBytes(&frame, length: MemoryLayout<FrameUniforms>.stride, index: 1)
            // Each translucent stroke marks its pixels with its own stencil
            // value (1...255, then around again; a stale equal value only
            // costs a fringe pixel).
            var translucentCount = 0
            for stroke in strokes {
                encoder.setVertexBuffer(stroke.segments, offset: 0, index: 0)
                if stroke.isTranslucent {
                    encoder.setStencilReferenceValue(UInt32(translucentCount % 255 + 1))
                    translucentCount += 1
                    var uniforms = stroke.uniforms
                    uniforms.interiorOnly = 1
                    draw(stroke, uniforms, with: interior, in: encoder)
                    uniforms.interiorOnly = 0
                    draw(stroke, uniforms, with: fringe, in: encoder)
                } else {
                    draw(stroke, stroke.uniforms, with: plain, in: encoder)
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

    // MARK: MTKViewDelegate

    func mtkView(_ view: MTKView, drawableSizeWillChange size: CGSize) {
        onNeedsDisplay?()
    }

    func draw(in view: MTKView) {
        guard let pass = view.currentRenderPassDescriptor, view.bounds.width > 0 else { return }
        let pixelsPerPoint = Float(view.drawableSize.width / view.bounds.width)
        renderFrame(into: pass, pixelsPerPoint: pixelsPerPoint, presenting: view.currentDrawable)
    }

    // MARK: Flattening

    /// Brings `strokes` up to date with the geometry and the zoom: rebuilds
    /// the Elements that changed or were flattened for another power-of-two
    /// zoom level, reuses the others.
    private func refreshStrokes(scale: Float) {
        let level = Int(log2(max(scale, .leastNormalMagnitude)).rounded(.up))
        guard strokesAreStale || level != flatteningLevel, let geometry else { return }
        strokesAreStale = false
        flatteningLevel = level
        if geometry.generation() != cacheGeneration {
            // Element handles may mean other Elements now.
            cache.removeAll()
            cacheGeneration = geometry.generation()
        }
        let tolerance = Self.flatteningTolerancePixels / exp2(Float(level))

        var kept: [UInt32: CachedElement] = [:]
        var all: [StrokeDraw] = []
        geometry.withBuffers { buffers in
            var first = 0
            while first < buffers.parts.count {
                // The parts of one Element follow each other.
                let (element, version) = (buffers.parts[first].element, buffers.parts[first].version)
                var end = first + 1
                while end < buffers.parts.count, buffers.parts[end].element == element { end += 1 }

                var entry = cache[element]
                if entry?.version != version || entry?.level != level {
                    let parts = buffers.parts[first..<end]
                    entry = CachedElement(
                        version: version, level: level,
                        draws: parts.compactMap { strokeDraw(for: $0, in: buffers, tolerance: tolerance) })
                    elementsBuilt += 1
                }
                if let entry {
                    kept[element] = entry
                    all += entry.draws
                }
                first = end
            }
        }
        cache = kept
        strokes = all
    }

    private func strokeDraw(for part: GeometryPart, in buffers: GeometryBuffers, tolerance: Float) -> StrokeDraw? {
        guard part.kind == GeometryPart.strokeKind else { return nil }
        let segments = StrokeFlattener.segments(of: part, in: buffers, tolerance: tolerance)
        guard !segments.isEmpty,
            let buffer = segments.withUnsafeBytes({
                device.makeBuffer(bytes: $0.baseAddress!, length: $0.count, options: .storageModeShared)
            })
        else { return nil }
        let color = premultipliedColor(of: part)
        return StrokeDraw(
            segments: buffer, segmentCount: segments.count,
            uniforms: PartUniforms(color: color, width: part.width), isTranslucent: color.w < 1)
    }

    private func premultipliedColor(of part: GeometryPart) -> SIMD4<Float> {
        func channel(_ shift: UInt32) -> Float { Float((part.color >> shift) & 0xff) / 255 }
        let alpha = channel(0) * part.opacity
        return SIMD4(channel(24) * alpha, channel(16) * alpha, channel(8) * alpha, alpha)
    }
}
