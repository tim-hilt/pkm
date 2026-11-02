import Testing

@testable import WorkingTitle

/// The zero-copy geometry path: the client reads the core's buffers in place.
@Suite struct GeometryBuffersTests {
    @Test func tracerSceneIsOneStrokedPartReadInPlace() {
        Editor().geometry().withBuffers { buffers in
            #expect(buffers.parts.count == 1)
            let part = buffers.parts[0]
            #expect(part.kind == GeometryPart.strokeKind)
            #expect(part.color == 0x1e1e_1eff)
            #expect(part.width == 2)
            #expect(part.opacity == 1)
            #expect(part.version == 1)
            #expect(Int(part.verbCount) == buffers.verbs.count)

            #expect(buffers.verbs.first == GeometryVerb.move.rawValue)
            #expect(Array(buffers.coords.prefix(2)) == [82, 100])
        }
    }

    @Test func eachPartExposesItsOwnVerbs() {
        Editor().geometry().withBuffers { buffers in
            let verbs = buffers.verbs(of: buffers.parts[0])
            #expect(
                verbs.map { GeometryVerb(rawValue: $0) } == [
                    .move, .line, .cubic, .line, .cubic, .line, .cubic, .line, .cubic,
                ])
        }
    }
}
