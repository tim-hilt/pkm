import Testing

@testable import ExcalidrawNative

/// Curves are flattened on the client; expected values come from the circle
/// the rounded rectangle's corners approximate, not from the flattener's maths.
/// The buffers are written by hand so that the expected geometry is exact.
@Suite struct StrokeFlattenerTests {
    /// Flattens a rectangle (50,100)-(330,280) with corner radius 32, built
    /// the way the core describes rounded shapes: lines and one cubic per corner.
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
            verbStart: 0, verbCount: UInt32(verbs.count), coordStart: 0, dashOn: 0, dashOff: 0)
        return flatten(part, verbs: verbs, coords: coords, tolerance: tolerance)
    }

    private func flatten(_ part: GeometryPart, verbs: [UInt8], coords: [Float], tolerance: Float) -> [Segment] {
        verbs.withUnsafeBufferPointer { verbs in
            coords.withUnsafeBufferPointer { coords in
                withUnsafePointer(to: part) { part in
                    let buffers = GeometryBuffers(
                        parts: UnsafeBufferPointer(start: part, count: 1), verbs: verbs, coords: coords)
                    return StrokeFlattener.segments(of: part.pointee, in: buffers, tolerance: tolerance)
                }
            }
        }
    }

    /// Flattens the path made of `verbs` (0 move, 1 line) drawn dashed 8 on, 10 off.
    private func dashed(_ verbs: [UInt8], _ coords: [Float], on: Float = 8, off: Float = 10) -> [Segment] {
        let part = GeometryPart(
            element: 0, version: 1, kind: GeometryPart.strokeKind, color: 0x1e1e_1eff, opacity: 1, width: 2,
            verbStart: 0, verbCount: UInt32(verbs.count), coordStart: 0, dashOn: on, dashOff: off)
        return flatten(part, verbs: verbs, coords: coords, tolerance: 0.1)
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

    @Test func aDashedLineIsDrawnInStretchesOfTheOnLengthSeparatedByTheOffLength() {
        // 0-8 on, 8-18 off, 18-26 on, ... the last stretch 90-98 ends before the line does.
        let segments = dashed([0, 1], [0, 0, 100, 0])

        #expect(
            segments == [
                Segment(0, 0, 8, 0), Segment(18, 0, 26, 0), Segment(36, 0, 44, 0),
                Segment(54, 0, 62, 0), Segment(72, 0, 80, 0), Segment(90, 0, 98, 0),
            ])
    }

    @Test func aDashRunsOnAroundACorner() {
        // 5 along the first leg, then 3 more on the second: 8 in all.
        let segments = dashed([0, 1, 1], [0, 0, 5, 0, 5, 10])

        #expect(segments == [Segment(0, 0, 5, 0), Segment(5, 0, 5, 3)])
    }

    @Test func aGapRunsOnAroundACornerToo() {
        // The first stretch ends at 8 on a 10 long leg, the gap then ends at 18:
        // 8 into the second leg.
        let segments = dashed([0, 1, 1], [0, 0, 10, 0, 10, 100])

        #expect(segments.prefix(2) == [Segment(0, 0, 8, 0), Segment(10, 8, 10, 16)])
    }

    @Test func everyMoveStartsTheDashPatternAgain() {
        let segments = dashed([0, 1, 0, 1], [0, 0, 10, 0, 0, 5, 10, 5])

        #expect(
            segments == [
                Segment(0, 0, 8, 0), Segment(0, 5, 8, 5),
            ])
    }

    @Test func aDottedLineHasShortDotsFarApart() {
        let segments = dashed([0, 1], [0, 0, 30, 0], on: 1.5, off: 8)

        #expect(
            segments == [
                Segment(0, 0, 1.5, 0), Segment(9.5, 0, 11, 0), Segment(19, 0, 20.5, 0), Segment(28.5, 0, 30, 0),
            ])
    }

    @Test func aDashedCurveIsCutAlongItsLength() {
        // The same cubic as the first corner of the rounded rectangle, a
        // quarter circle of radius 32 of length 16 * pi = 50.27: a dash of
        // 8 and a gap of 10 give three dashes (0-8, 18-26, 36-44).
        let k: Float = 32 * 0.552_284_8
        let segments = dashed([0, 2], [0, 32, 0, 32 - k, 32 - k, 0, 32, 0])
        var length: Float = 0
        var dashes = 0
        var previousEnd: SIMD2<Float>?
        for segment in segments {
            let start = SIMD2(segment.x, segment.y)
            let end = SIMD2(segment.z, segment.w)
            length += (end - start).length()
            if previousEnd != start { dashes += 1 }
            previousEnd = end
        }

        #expect(dashes == 3)
        // Chords undercut the arc a little: 3 * 8 = 24, within a percent.
        #expect(abs(length - 24) < 0.3)
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
