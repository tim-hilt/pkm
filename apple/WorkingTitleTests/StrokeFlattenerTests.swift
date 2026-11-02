import Testing

@testable import WorkingTitle

/// Curves are flattened on the client; expected values come from the circle
/// the tracer rectangle's corners approximate, not from the flattener's maths.
@Suite struct StrokeFlattenerTests {
    /// Flattens the tracer rectangle (50,100)-(330,280) with corner radius 32.
    private func flattenedRectangle(tolerance: Float) -> [Segment] {
        Editor().geometry().withBuffers { buffers in
            StrokeFlattener.segments(of: buffers.parts[0], in: buffers, tolerance: tolerance)
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
