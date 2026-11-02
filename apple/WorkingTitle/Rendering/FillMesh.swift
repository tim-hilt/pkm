/// A fill part prepared for stencil-then-cover (ADR-0002): triangles that
/// the GPU rasterises into the stencil buffer to find out which pixels the
/// path encloses, and a rectangle that then paints exactly those.
struct FillMesh {
    /// Vertices of the fan triangles, three per triangle, in Scene
    /// coordinates. Every subpath is closed implicitly and fanned out from its
    /// first point, so the signed triangles add up to the path's winding
    /// number at every point, whatever the shape: concave, with holes, or
    /// crossing itself.
    private(set) var triangles: [SIMD2<Float>] = []
    /// Two triangles covering everything the fill can touch.
    private(set) var cover: [SIMD2<Float>] = []

    /// The mesh of `part`, or nil if its path encloses no area.
    static func mesh(of part: GeometryPart, in buffers: GeometryBuffers, tolerance: Float) -> FillMesh? {
        var builder = Builder()
        PathFlattener.flatten(part, in: buffers, tolerance: tolerance, into: &builder)
        return builder.finish()
    }

    private struct Builder: PathSink {
        var mesh = FillMesh()
        var lower = SIMD2<Float>(repeating: .infinity)
        var upper = SIMD2<Float>(repeating: -.infinity)
        /// First and latest point of the subpath, and how many it has.
        var anchor = SIMD2<Float>.zero
        var last = SIMD2<Float>.zero
        var pointCount = 0

        mutating func beginSubpath(at point: SIMD2<Float>) {
            anchor = point
            last = point
            pointCount = 1
            include(point)
        }

        mutating func addLine(from start: SIMD2<Float>, to end: SIMD2<Float>) {
            if pointCount >= 2 {
                mesh.triangles += [anchor, last, end]
            }
            last = end
            pointCount += 1
            include(end)
        }

        private mutating func include(_ point: SIMD2<Float>) {
            lower = pointwiseMin(lower, point)
            upper = pointwiseMax(upper, point)
        }

        /// The cover is a little larger than the path, so that no pixel the
        /// path reaches into lies outside it.
        func finish() -> FillMesh? {
            guard !mesh.triangles.isEmpty else { return nil }
            var mesh = mesh
            let (low, high) = (lower - 1, upper + 1)
            let (a, b) = (SIMD2(low.x, low.y), SIMD2(high.x, low.y))
            let (c, d) = (SIMD2(high.x, high.y), SIMD2(low.x, high.y))
            mesh.cover = [a, b, c, a, c, d]
            return mesh
        }
    }
}
