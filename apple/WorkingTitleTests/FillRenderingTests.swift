import Foundation
import Testing

@testable import WorkingTitle

/// Renders fills offscreen and probes pixels. Fill colour #a5d8ff has a red
/// channel of 165, paper is 255 and ink 30, so the red channel tells them apart.
@MainActor @Suite struct FillRenderingTests {
    private let fillRed = 165

    private func render(_ geometry: any GeometrySource, size: Int = 400) throws -> RenderedImage {
        try OffscreenRender.render(geometry, viewport: Viewport(), size: size).image
    }

    private func red(_ frame: RenderedImage, _ x: Int, _ y: Int) -> Int { Int(frame.red(x, y)) }

    // MARK: Fills from the core

    private let filledRectangle = ##","backgroundColor":"#a5d8ff","fillStyle":"solid""##

    @Test func aSolidBackgroundFillsTheShapeAndNothingElse() throws {
        let frame = try render(TestScenes.rectangle(extra: filledRectangle))

        // Rectangle (50,100)-(330,280).
        #expect(red(frame, 200, 190) == fillRed, "inside")
        #expect(red(frame, 70, 120) == fillRed, "near the corner")
        #expect(red(frame, 20, 20) == 255, "outside")
        #expect(red(frame, 200, 295) == 255, "below")
        #expect(red(frame, 200, 100) < 60, "the outline lies over the fill")
    }

    @Test func theBackgroundColourIsTheFillColour() throws {
        let frame = try render(
            TestScenes.rectangle(extra: ##","backgroundColor":"#e03131","fillStyle":"solid""##))

        // #e03131: red 224.
        #expect(red(frame, 200, 190) == 224)
    }

    @Test func aTransparentBackgroundLeavesTheInsidePaper() throws {
        let frame = try render(TestScenes.rectangle(extra: #","backgroundColor":"transparent""#))

        #expect(red(frame, 200, 190) == 255)
    }

    @Test func fillsAreAsOpaqueAsTheElement() throws {
        let frame = try render(TestScenes.rectangle(extra: filledRectangle + #","opacity":50"#))

        // Half of 165 over half of paper 255, blended once.
        #expect((205...215).contains(red(frame, 200, 190)), "\(red(frame, 200, 190))")
    }

    @Test func hatchingDrawsLinesOfTheBackgroundColourWithPaperBetween() throws {
        let frame = try render(
            TestScenes.rectangle(extra: ##","backgroundColor":"#a5d8ff","fillStyle":"hachure""##))

        // Lines every 8 units of 1 unit thickness: some pixels in a row are
        // fill coloured, some are paper, none are ink.
        let row = (100...280).map { red(frame, $0, 190) }
        #expect(row.contains { $0 < 200 }, "has line pixels")
        #expect(row.contains(255), "has paper between the lines")
        #expect(row.allSatisfy { $0 >= 120 }, "no ink")
    }

    // MARK: Winding rules

    private let triangle: [SIMD2<Float>] = [[50, 50], [350, 50], [200, 350]]
    /// A pentagram drawn as one self-intersecting path: the centre pentagon
    /// is wound around twice.
    private let star: [SIMD2<Float>] = (0..<5).map { i in
        let angle = Float(i) * 4 * .pi / 5 - .pi / 2
        return SIMD2(200 + 180 * cos(angle), 200 + 180 * sin(angle))
    }

    @Test func concaveOutlinesAreFilledOnlyInside() throws {
        // An L: the notch at the top right stays paper.
        let ell: [SIMD2<Float>] = [[50, 50], [150, 50], [150, 250], [350, 250], [350, 350], [50, 350]]
        let frame = try render(HandBuiltGeometry(.fill(ell)))

        #expect(red(frame, 100, 100) == fillRed, "upright")
        #expect(red(frame, 250, 300) == fillRed, "foot")
        #expect(red(frame, 250, 100) == 255, "the notch")
        #expect(red(frame, 320, 200) == 255, "above the foot")
    }

    @Test func nonZeroFillsTheMiddleOfAStarTheEvenOddRuleLeavesItEmpty() throws {
        let nonZero = try render(HandBuiltGeometry(.fill(star, rule: .nonZero)))
        let evenOdd = try render(HandBuiltGeometry(.fill(star, rule: .evenOdd)))

        for (name, frame) in [("non-zero", nonZero), ("even-odd", evenOdd)] {
            #expect(red(frame, 200, 40) == fillRed, "\(name): the top point")
            #expect(red(frame, 20, 20) == 255, "\(name): outside")
        }
        #expect(red(nonZero, 200, 200) == fillRed, "non-zero: the middle is wound twice, inside")
        #expect(red(evenOdd, 200, 200) == 255, "even-odd: the middle is a hole")
    }

    @Test func opposingSubpathsMakeAHoleOnlyUnderNonZero() throws {
        // Outer square clockwise (y down); the inner one counter-clockwise
        // cancels it out, the inner one in the same direction does not.
        let outer: [SIMD2<Float>] = [[50, 50], [350, 50], [350, 350], [50, 350]]
        let opposite: [SIMD2<Float>] = [[150, 150], [150, 250], [250, 250], [250, 150]]
        let same: [SIMD2<Float>] = opposite.reversed()

        let hole = try render(HandBuiltGeometry(.fill(subpaths: [outer, opposite])))
        let filled = try render(HandBuiltGeometry(.fill(subpaths: [outer, same])))
        let evenOddFilled = try render(HandBuiltGeometry(.fill(subpaths: [outer, same], rule: .evenOdd)))

        #expect(red(hole, 200, 200) == 255, "opposite direction: hole")
        #expect(red(hole, 100, 100) == fillRed, "opposite direction: ring")
        #expect(red(filled, 200, 200) == fillRed, "same direction: filled")
        #expect(red(evenOddFilled, 200, 200) == 255, "even-odd: hole either way")
    }

    @Test func aTranslucentFillBlendsOnceWhereItOverlapsItself() throws {
        // The star's middle is wound twice; it must not look darker than the tips.
        let frame = try render(HandBuiltGeometry(.fill(star, opacity: 0.5)))

        let tip = red(frame, 200, 40)
        #expect((205...215).contains(tip), "\(tip)")
        #expect(red(frame, 200, 200) == tip, "the middle")
    }

    // MARK: Fills and strokes together

    @Test func aFillPaintsOverTheMarksOfEarlierTranslucentStrokes() throws {
        // The translucent stroke leaves stencil marks; the triangle drawn
        // after it must be filled completely, also where they are.
        let geometry = HandBuiltGeometry(
            .stroke([[50, 200], [350, 200]], width: 40, opacity: 0.5),
            .fill(triangle))
        let frame = try render(geometry)

        #expect(red(frame, 200, 200) == fillRed, "over the stroke")
        #expect(red(frame, 200, 100) == fillRed, "beside it")
        #expect(red(frame, 100, 300) == 255, "outside")
    }

    @Test func marksOfTranslucentStrokesDoNotLeakIntoALaterFillAfterAnotherFill() throws {
        // The fill in between lies away from the stroke; the triangle after it
        // must still count its winding from zero where the stroke was.
        let elsewhere: [SIMD2<Float>] = [[300, 300], [350, 300], [350, 350], [300, 350]]
        let lowerLeft: [SIMD2<Float>] = [[50, 50], [50, 350], [350, 350]]
        let geometry = HandBuiltGeometry(
            .stroke([[60, 100], [340, 100]], width: 40, opacity: 0.5),
            .fill(elsewhere),
            .fill(lowerLeft))
        let frame = try render(geometry)

        // (300, 100) is on the stroke, inside the triangle's cover rectangle
        // but outside the triangle: half-opaque ink over paper.
        #expect((135...150).contains(red(frame, 300, 100)), "outside the triangle: \(red(frame, 300, 100))")
        #expect(red(frame, 100, 300) == fillRed, "inside the triangle")
    }

    @Test func everyTranslucentStrokeIsDrawnHoweverManyCameBefore() throws {
        // More translucent strokes than the stencil has values: the last one
        // crosses the first and must blend over it like any other Element.
        let filler = (0..<300).map { _ in
            HandBuiltPart.stroke([[10, 380], [12, 380]], width: 1, opacity: 0.5)
        }
        let first = HandBuiltPart.stroke([[50, 200], [350, 200]], width: 40, opacity: 0.5)
        let last = HandBuiltPart.stroke([[200, 50], [200, 350]], width: 40, opacity: 0.5)
        for count in [254, 255, 256] {
            let frame = try render(HandBuiltGeometry([first] + filler.prefix(count) + [last]))
            // Ink at half opacity twice over paper: 255 -> 142 -> 86.
            #expect((80...92).contains(red(frame, 200, 200)), "after \(count): \(red(frame, 200, 200))")
        }
    }

    @Test func aTranslucentStrokeIsFullyDrawnOverAnEarlierFill() throws {
        let geometry = HandBuiltGeometry(
            .fill(triangle),
            .stroke([[50, 200], [350, 200]], width: 40, opacity: 0.5))
        let frame = try render(geometry)

        // Ink at half opacity over the fill: (30 + 165) / 2 = 97.
        #expect((92...102).contains(red(frame, 200, 200)), "\(red(frame, 200, 200))")
        #expect((135...150).contains(red(frame, 360, 200)), "over paper: \(red(frame, 360, 200))")
    }

    @Test func consecutiveFillsDoNotLeakIntoEachOther() throws {
        // Two overlapping squares with the opposite winding; the second's
        // stencil pass must start from zero where the first one was.
        let a: [SIMD2<Float>] = [[50, 50], [250, 50], [250, 250], [50, 250]]
        let b: [SIMD2<Float>] = [[150, 150], [150, 350], [350, 350], [350, 150]]
        let frame = try render(HandBuiltGeometry(.fill(a, color: 0xe031_31ff), .fill(b)))

        #expect(red(frame, 100, 100) == 224, "only the first")
        #expect(red(frame, 200, 200) == fillRed, "the second over the first")
        #expect(red(frame, 300, 300) == fillRed, "only the second")
    }

    @Test func fillsAreCachedLikeStrokesAndReflattenedWhenZoomingToAnotherLevel() throws {
        let renderer = try OffscreenRender.makeRenderer()
        renderer.setGeometry(HandBuiltGeometry(.fill(triangle)))
        _ = OffscreenRender.draw(with: renderer)
        renderer.viewport = Viewport(origin: [30, 30], zoom: 1)
        _ = OffscreenRender.draw(with: renderer)
        #expect(renderer.elementsBuilt == 1, "panned")

        renderer.viewport = Viewport(origin: [30, 30], zoom: 4)
        _ = OffscreenRender.draw(with: renderer)
        #expect(renderer.elementsBuilt == 2, "zoomed in")
    }
}
