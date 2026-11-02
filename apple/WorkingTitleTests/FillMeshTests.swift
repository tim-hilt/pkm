import Testing

@testable import WorkingTitle

/// A fill's path becomes triangle fans for the stencil pass plus a covering
/// rectangle. Paths are written by hand so that expected vertices are exact.
@Suite struct FillMeshTests {
    private func mesh(of part: HandBuiltPart, tolerance: Float = 0.1) -> FillMesh? {
        HandBuiltGeometry(part).withBuffers { buffers in
            FillMesh.mesh(of: buffers.parts[0], in: buffers, tolerance: tolerance)
        }
    }

    @Test func aSquareIsTwoTrianglesFannedFromItsFirstCorner() throws {
        let square: [SIMD2<Float>] = [[10, 20], [110, 20], [110, 80], [10, 80]]

        let mesh = try #require(mesh(of: .fill(square)))

        #expect(mesh.triangles == [[10, 20], [110, 20], [110, 80], [10, 20], [110, 80], [10, 80]])
    }

    @Test func theCoverIsTheBoundsGrownByOneUnitAsTwoTriangles() throws {
        let mesh = try #require(mesh(of: .fill([[10, 20], [110, 20], [110, 80], [10, 80]])))

        #expect(mesh.cover == [[9, 19], [111, 19], [111, 81], [9, 19], [111, 81], [9, 81]])
    }

    @Test func everySubpathIsFannedFromItsOwnFirstPoint() throws {
        let outer: [SIMD2<Float>] = [[0, 0], [100, 0], [100, 100]]
        let inner: [SIMD2<Float>] = [[200, 0], [300, 0], [300, 100]]

        let mesh = try #require(mesh(of: .fill(subpaths: [outer, inner])))

        #expect(mesh.triangles == outer + inner)
    }

    @Test func aPathWithoutAreaHasNoMesh() {
        #expect(mesh(of: .fill([[0, 0], [10, 10]])) == nil, "a line")
        #expect(mesh(of: .fill([[5, 5]])) == nil, "a point")
        #expect(mesh(of: .fill([])) == nil, "nothing")
    }

    @Test func curvesAreFlattenedWithinTheTolerance() throws {
        // A quarter disc of radius 100: a cubic arc and the two radii.
        let k: Float = 55.228_474_98
        let part = HandBuiltPart(
            path: [.move(0, 0), .line(100, 0), .cubic(100, k, k, 100, 0, 100)])

        let mesh = try #require(mesh(of: part, tolerance: 0.05))

        #expect(mesh.triangles.count > 3 * 8, "an arc needs several chords")
        // Every vertex lies on the radii or on the circle.
        for vertex in mesh.triangles {
            let radius = (vertex * vertex).sum().squareRoot()
            let onRadius = vertex.x == 0 || vertex.y == 0
            #expect(onRadius || abs(radius - 100) < 0.1, "\(vertex)")
        }
    }
}
