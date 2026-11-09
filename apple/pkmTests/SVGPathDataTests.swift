import Testing

@testable import pkm

@Suite struct SVGPathDataTests {
    private func ops(_ data: String) -> [String] {
        let path = SVGPathData(data)
        var coords = path.coords[...]
        return path.verbs.map { verb in
            let count = GeometryVerb(rawValue: verb) == .cubic ? 6 : 2
            defer { coords = coords.dropFirst(count) }
            let name = ["move", "line", "cubic"][Int(verb)]
            return "\(name) " + coords.prefix(count).map { "\($0)" }.joined(separator: " ")
        }
    }

    @Test func absoluteCommandsGoWhereTheySay() {
        #expect(ops("M10 20L30 40") == ["move 10.0 20.0", "line 30.0 40.0"])
    }

    @Test func relativeCommandsStartFromTheCurrentPoint() {
        #expect(ops("M10 20l5 5l-5 0") == ["move 10.0 20.0", "line 15.0 25.0", "line 10.0 25.0"])
    }

    @Test func horizontalAndVerticalLinesKeepTheOtherCoordinate() {
        #expect(
            ops("M10 20H30V50h-5v-10") == [
                "move 10.0 20.0", "line 30.0 20.0", "line 30.0 50.0", "line 25.0 50.0", "line 25.0 40.0",
            ])
    }

    @Test func numbersMayRunTogetherAtASignOrASecondDot() {
        #expect(ops("M1-2L.5.25") == ["move 1.0 -2.0", "line 0.5 0.25"])
    }

    @Test func furtherCoordinatePairsRepeatTheCommandAndAMoveContinuesWithLines() {
        #expect(ops("M0 0 10 0 10 10") == ["move 0.0 0.0", "line 10.0 0.0", "line 10.0 10.0"])
        #expect(ops("M0 0l1 1 1 1") == ["move 0.0 0.0", "line 1.0 1.0", "line 2.0 2.0"])
    }

    @Test func cubicCurvesTakeTwoControlPointsAndAnEnd() {
        #expect(ops("M0 0c1 2 3 4 5 6") == ["move 0.0 0.0", "cubic 1.0 2.0 3.0 4.0 5.0 6.0"])
    }

    @Test func smoothCurvesReflectTheLastControlPoint() {
        // The first control point of the second curve mirrors (3, 4) about
        // the end (5, 6): (7, 8).
        #expect(
            ops("M0 0C1 2 3 4 5 6S9 9 10 10") == [
                "move 0.0 0.0", "cubic 1.0 2.0 3.0 4.0 5.0 6.0", "cubic 7.0 8.0 9.0 9.0 10.0 10.0",
            ])
    }

    @Test func closingReturnsToTheStartOfTheSubpathForRelativeCommandsThatFollow() {
        #expect(
            ops("M10 10l5 0 0 5zm1 1l2 0") == [
                "move 10.0 10.0", "line 15.0 10.0", "line 15.0 15.0", "move 11.0 11.0", "line 13.0 11.0",
            ])
        // A drawing command right after z starts a new subpath at the start.
        #expect(ops("M10 10l5 0zl0 5") == ["move 10.0 10.0", "line 15.0 10.0", "move 10.0 10.0", "line 10.0 15.0"])
    }

    @Test func aPathItCannotReadParsesAsFarAsItCan() {
        #expect(ops("M1 2L3 4A5 5 0 0 1 9 9L7 7") == ["move 1.0 2.0", "line 3.0 4.0"])
        #expect(ops("") == [])
    }
}
