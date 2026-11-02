import Testing

@testable import WorkingTitle

/// Curves are flattened on the client; expected values come from the circle
/// the rounded rectangle's corners approximate, not from the flattener's maths.
/// The core emits no curves yet, so the buffers are written by hand here.
@Suite struct StrokeFlattenerTests {
    /// Flattens a rectangle (50,100)-(330,280) with corner radius 32, built
    /// the way the core will describe rounded shapes: lines and one cubic per corner.
    private func flattenedRectangle(tolerance: Float) -> [Segment] {
        let (x, y, right, bottom, r): (Float, Float, Float, Float, Float) = (50, 100, 330, 280, 32)
        let k = r * 0.552_284_8  // control-point distance of a quarter circle
        let verbs: [UInt8] = [0, 1, 2, 1, 2, 1, 2, 1, 2]
        let coords: [Float] = [
            x + r, y,
            right - r, y,
            right - r + k, y, right, y + r - k, right, y + r,
            right, bottom - r,
            right, bottom - r + k, right - r + k, bottom, right - r, bottom,
            x + r, bottom,
            x + r - k, bottom, x, bottom - r + k, x, bottom - r,
            x, y + r,
            x, y + r - k, x + r - k, y, x + r, y,
        ]
        let part = GeometryPart(
            element: 0, version: 1, kind: GeometryPart.strokeKind, color: 0x1e1e_1eff, opacity: 1, width: 2,
            verbStart: 0, verbCount: UInt32(verbs.count), coordStart: 0)
        return verbs.withUnsafeBufferPointer { verbs in
            coords.withUnsafeBufferPointer { coords in
                withUnsafePointer(to: part) { part in
                    let buffers = GeometryBuffers(
                        parts: UnsafeBufferPointer(start: part, count: 1), verbs: verbs, coords: coords)
                    return StrokeFlattener.segments(of: part.pointee, in: buffers, tolerance: tolerance)
                }
            }
        }
    }

    @Test func straightEdgesStaySingleSegments() {
        let segments = flattenedRectangle(tolerance: 0.25)
        // Top edge is the first line after the move.
        #expect(segments[0] == Segment(82, 100, 298, 100))
    }

    @Test func flatteningStaysWithinToleranceOfTheCorner() {
        let tolerance: Float = 0.05
        let segments = flattenedRectangle(tolerance: tolerance)
        let corner = SIMD2<Float>(298, 132)  // centre of the top-right corner arc

        let arc = segments.filter { $0.x >= 298 && $0.z <= 330 && $0.y <= 132 && $0.w <= 132 }
        #expect(arc.count > 3)
        for segment in arc {
            let midpoint = (SIMD2(segment.x, segment.y) + SIMD2(segment.z, segment.w)) / 2
            let distance = (midpoint - corner).length()
            // Chord midpoints lie just inside the circle; allow the cubic's own
            // 0.03% radius error on top of the requested tolerance.
            #expect(distance <= 32 + 0.02)
            #expect(distance >= 32 - tolerance - 0.02)
        }
    }

    @Test func finerToleranceYieldsMoreSegments() {
        let coarse = flattenedRectangle(tolerance: 1)
        let fine = flattenedRectangle(tolerance: 0.01)
        #expect(fine.count > coarse.count)
    }

    @Test func outlineIsContinuousAndClosed() {
        let segments = flattenedRectangle(tolerance: 0.1)
        for (previous, next) in zip(segments, segments.dropFirst()) {
            #expect(SIMD2(previous.z, previous.w) == SIMD2(next.x, next.y))
        }
        #expect(SIMD2(segments[0].x, segments[0].y) == SIMD2(segments.last!.z, segments.last!.w))
    }
}

extension SIMD2<Float> {
    fileprivate func length() -> Float { (self * self).sum().squareRoot() }
}
