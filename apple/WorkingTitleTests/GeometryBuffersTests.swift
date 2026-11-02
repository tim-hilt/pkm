import Testing

@testable import WorkingTitle

/// The zero-copy geometry path: the client reads the core's buffers in place.
@Suite struct GeometryBuffersTests {
    @Test func aRectangleIsOneStrokedPartReadInPlace() {
        TestScenes.rectangle().withBuffers { buffers in
            #expect(buffers.parts.count == 1)
            let part = buffers.parts[0]
            #expect(part.kind == GeometryPart.strokeKind)
            #expect(part.color == 0x1e1e_1eff)
            #expect(part.width == 2)
            #expect(part.opacity == 1)
            #expect(part.version == 1)
            #expect(Int(part.verbCount) == buffers.verbs.count)

            #expect(buffers.verbs.first == GeometryVerb.move.rawValue)
            #expect(Array(buffers.coords.prefix(2)) == [50, 100])
            #expect(part.dash == nil)
        }
    }

    @Test func eachPartExposesItsOwnVerbs() {
        TestScenes.rectangle().withBuffers { buffers in
            let verbs = buffers.verbs(of: buffers.parts[0])
            // Four edges, each a move and a curve, drawn twice.
            #expect(
                verbs.map { GeometryVerb(rawValue: $0) } == Array(repeating: [.move, .cubic], count: 8).flatMap { $0 })
        }
    }

    @Test func aDashedStrokeCarriesItsDashPattern() {
        TestScenes.rectangle(extra: #","strokeStyle":"dashed""#).withBuffers { buffers in
            let part = buffers.parts[0]
            #expect(part.width == 2.5)
            #expect(part.dash?.on == 8)
            #expect(part.dash?.off == 10)
        }
    }

    @Test func aSolidFilledRectangleIsAFillPartThenAStrokePart() {
        TestScenes.rectangle(extra: ##","backgroundColor":"#a5d8ff","fillStyle":"solid""##).withBuffers { buffers in
            #expect(buffers.parts.count == 2)
            let fill = buffers.parts[0]
            #expect(fill.kind == GeometryPart.fillKind)
            #expect(fill.color == 0xa5d8_ffff)
            #expect(fill.rule == .nonZero)
            #expect(fill.width == 0)
            #expect(fill.dash == nil)
            #expect(buffers.parts[1].kind == GeometryPart.strokeKind)
            #expect(buffers.parts[1].rule == .nonZero, "strokes carry rule 0")
        }
    }

    @Test func shapesMadeWithAPolygonAreFilledByTheEvenOddRule() {
        TestScenes.geometry(
            of:
                ##"{"type":"excalidraw","version":2,"elements":[{"id":"d","type":"diamond","width":80,"height":80,"backgroundColor":"#a5d8ff","fillStyle":"solid"}]}"##
        ).withBuffers { buffers in
            #expect(buffers.parts[0].rule == .evenOdd)
        }
    }
}
