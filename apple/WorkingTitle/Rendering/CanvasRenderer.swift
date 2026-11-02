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
/// Known limitation: overlapping capsules blend twice, so a stroke with
/// opacity below 1 shows darker joins until a later ticket resolves it.
@MainActor
final class CanvasRenderer: NSObject, MTKViewDelegate {
    /// Samples per pixel; the render target must be multisampled with this count.
    static let sampleCount = 4
    static let pixelFormat = MTLPixelFormat.bgra8Unorm

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
    private let device: any MTLDevice

    private(set) var geometry: Geometry?
    private var strokes: [StrokeDraw] = []
    /// Power-of-two zoom level `strokes` were flattened for; curves are only
    /// re-flattened when it changes, not on every zoom step.
    private var flatteningLevel: Int?

    /// A part's segments on the GPU plus how to draw them.
    private struct StrokeDraw {
        var segments: any MTLBuffer
        var segmentCount: Int
        var uniforms: PartUniforms
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
        color.pixelFormat = Self.pixelFormat
        // Premultiplied source-over.
        color.isBlendingEnabled = true
        color.sourceRGBBlendFactor = .one
        color.sourceAlphaBlendFactor = .one
        color.destinationRGBBlendFactor = .oneMinusSourceAlpha
        color.destinationAlphaBlendFactor = .oneMinusSourceAlpha
        guard let pipeline = try? device.makeRenderPipelineState(descriptor: descriptor) else { return nil }

        self.device = device
        self.queue = queue
        self.pipeline = pipeline
    }

    /// Replaces what is drawn; the same object again is a no-op. Keeps
    /// `geometry` alive: curves are flattened from the core's buffers again
    /// whenever the zoom level changes.
    func setGeometry(_ geometry: Geometry) {
        guard geometry !== self.geometry else { return }
        self.geometry = geometry
        flatteningLevel = nil
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

        let scale = viewport.zoom * pixelsPerPoint
        refreshStrokes(scale: scale)

        if let encoder = commands.makeRenderCommandEncoder(descriptor: pass) {
            encoder.setRenderPipelineState(pipeline)
            var frame = FrameUniforms(
                origin: viewport.origin, viewSize: SIMD2(Float(target.width), Float(target.height)), scale: scale)
            encoder.setVertexBytes(&frame, length: MemoryLayout<FrameUniforms>.stride, index: 1)
            for stroke in strokes {
                var uniforms = stroke.uniforms
                encoder.setVertexBuffer(stroke.segments, offset: 0, index: 0)
                encoder.setVertexBytes(&uniforms, length: MemoryLayout<PartUniforms>.stride, index: 2)
                encoder.setFragmentBytes(&uniforms, length: MemoryLayout<PartUniforms>.stride, index: 2)
                encoder.drawPrimitives(
                    type: .triangleStrip, vertexStart: 0, vertexCount: 4, instanceCount: stroke.segmentCount)
            }
            encoder.endEncoding()
        }
        if let drawable { commands.present(drawable) }
        commands.commit()
        framesRendered += 1
        return commands
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

    /// Re-flattens the geometry if `scale` moved to another power-of-two level.
    private func refreshStrokes(scale: Float) {
        let level = Int(log2(max(scale, .leastNormalMagnitude)).rounded(.up))
        guard level != flatteningLevel, let geometry else { return }
        flatteningLevel = level
        let tolerance = Self.flatteningTolerancePixels / exp2(Float(level))
        strokes = geometry.withBuffers { buffers in
            buffers.parts.compactMap { part in
                guard part.kind == GeometryPart.strokeKind else { return nil }
                let segments = StrokeFlattener.segments(of: part, in: buffers, tolerance: tolerance)
                guard !segments.isEmpty,
                    let buffer = segments.withUnsafeBytes({
                        device.makeBuffer(bytes: $0.baseAddress!, length: $0.count, options: .storageModeShared)
                    })
                else { return nil }
                return StrokeDraw(
                    segments: buffer, segmentCount: segments.count,
                    uniforms: PartUniforms(color: premultipliedColor(of: part), width: part.width))
            }
        }
    }

    private func premultipliedColor(of part: GeometryPart) -> SIMD4<Float> {
        func channel(_ shift: UInt32) -> Float { Float((part.color >> shift) & 0xff) / 255 }
        let alpha = channel(0) * part.opacity
        return SIMD4(channel(24) * alpha, channel(16) * alpha, channel(8) * alpha, alpha)
    }
}
