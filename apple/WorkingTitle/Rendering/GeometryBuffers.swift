/// Swift view of the core's geometry output (ADR-0007). The format is
/// documented in the core's `geometry` module; this file mirrors it.

/// Mirror of the core's `Part` record, read straight out of the core's memory.
/// Field order and sizes must match; `withBuffers` checks the record size.
struct GeometryPart {
    /// The only kind so far: constant-width outline with round caps and joins.
    static let strokeKind: UInt32 = 0

    /// Core-assigned Element handle; with `version` the cache key.
    var element: UInt32
    var version: UInt32
    var kind: UInt32
    /// `0xRRGGBBAA`.
    var color: UInt32
    /// Element opacity, 0...1.
    var opacity: Float
    /// Line width in Scene units.
    var width: Float
    /// Range of this part's verbs in `GeometryBuffers.verbs`.
    var verbStart: UInt32
    var verbCount: UInt32
    /// Index of this part's first operand in `GeometryBuffers.coords`.
    var coordStart: UInt32
    /// Length of the drawn stretches and of the gaps of a dashed line, in
    /// Scene units; both 0 for a solid line.
    var dashOn: Float
    var dashOff: Float

    /// The dash pattern, if the part is dashed. Like a canvas' `setLineDash`
    /// it starts over at every `move`, and the round caps come on top of the
    /// dashes' length.
    var dash: (on: Float, off: Float)? {
        dashOn > 0 && dashOff > 0 ? (dashOn, dashOff) : nil
    }
}

/// Path operation numbers, as in the core's `Verb`.
enum GeometryVerb: UInt8 {
    /// Operands: x y.
    case move = 0
    /// Operands: x y.
    case line = 1
    /// Operands: x1 y1 x2 y2 x y (two control points, then the end point).
    case cubic = 2
}

/// The core's three geometry buffers, borrowed. Only valid inside the
/// `Geometry.withBuffers` call that produced them.
struct GeometryBuffers {
    let parts: UnsafeBufferPointer<GeometryPart>
    let verbs: UnsafeBufferPointer<UInt8>
    let coords: UnsafeBufferPointer<Float>

    /// The verbs of `part`.
    func verbs(of part: GeometryPart) -> UnsafeBufferPointer<UInt8> {
        UnsafeBufferPointer(rebasing: verbs[Int(part.verbStart)..<Int(part.verbStart + part.verbCount)])
    }

    /// The operands from the start of `part` to the end of the buffer; the
    /// part's verbs say how many it consumes.
    func coords(of part: GeometryPart) -> UnsafeBufferPointer<Float> {
        UnsafeBufferPointer(rebasing: coords[Int(part.coordStart)...])
    }
}

extension Geometry {
    /// Runs `body` with the core's buffers, read in place without copying.
    /// The buffers must not escape `body`: they belong to this object.
    func withBuffers<R>(_ body: (GeometryBuffers) throws -> R) rethrows -> R {
        precondition(
            MemoryLayout<GeometryPart>.stride == Int(partStride()),
            "GeometryPart no longer mirrors the core's Part record")
        return try withExtendedLifetime(self) {
            try body(
                GeometryBuffers(
                    parts: buffer(at: partsAddress(), count: partCount()),
                    verbs: buffer(at: verbsAddress(), count: verbCount()),
                    coords: buffer(at: coordsAddress(), count: coordCount())))
        }
    }
}

private func buffer<T>(at address: UInt64, count: UInt64) -> UnsafeBufferPointer<T> {
    UnsafeBufferPointer(start: UnsafePointer<T>(bitPattern: UInt(address)), count: Int(count))
}
