@testable import pkm

/// One path operation of hand-built geometry.
enum PathOp {
    case move(Float, Float)
    case line(Float, Float)
    case cubic(Float, Float, Float, Float, Float, Float)
}

/// A part written by hand, for tests that need paths the core does not
/// produce (yet): concave or self-intersecting outlines, exact winding.
struct HandBuiltPart {
    var kind = GeometryPart.fillKind
    var color: UInt32 = 0xa5d8_ffff
    var opacity: Float = 1
    var width: Float = 0
    var rule = GeometryFillRule.nonZero
    var path: [PathOp]

    /// A fill through `points`, closed implicitly.
    static func fill(
        _ points: [SIMD2<Float>], color: UInt32 = 0xa5d8_ffff, opacity: Float = 1,
        rule: GeometryFillRule = .nonZero
    ) -> HandBuiltPart {
        HandBuiltPart(
            color: color, opacity: opacity, rule: rule,
            path: points.enumerated().map {
                $0.offset == 0 ? .move($0.element.x, $0.element.y) : .line($0.element.x, $0.element.y)
            })
    }

    /// Several subpaths in one fill part.
    static func fill(
        subpaths: [[SIMD2<Float>]], color: UInt32 = 0xa5d8_ffff, rule: GeometryFillRule = .nonZero
    ) -> HandBuiltPart {
        var part = fill([], color: color, rule: rule)
        part.path = subpaths.flatMap { points in
            points.enumerated().map {
                $0.offset == 0 ? PathOp.move($0.element.x, $0.element.y) : .line($0.element.x, $0.element.y)
            }
        }
        return part
    }

    /// A stroked polyline through `points`.
    static func stroke(
        _ points: [SIMD2<Float>], width: Float = 2, color: UInt32 = 0x1e1e_1eff, opacity: Float = 1
    ) -> HandBuiltPart {
        HandBuiltPart(
            kind: GeometryPart.strokeKind, color: color, opacity: opacity, width: width,
            path: points.enumerated().map {
                $0.offset == 0 ? .move($0.element.x, $0.element.y) : .line($0.element.x, $0.element.y)
            })
    }
}

/// Geometry the renderer can draw without the core: every part is its own
/// Element, at version 1.
final class HandBuiltGeometry: GeometrySource {
    private var parts: [GeometryPart] = []
    private var verbs: [UInt8] = []
    private var coords: [Float] = []

    init(_ built: [HandBuiltPart]) {
        for (index, part) in built.enumerated() {
            let (verbStart, coordStart) = (verbs.count, coords.count)
            for op in part.path {
                switch op {
                case .move(let x, let y):
                    verbs.append(GeometryVerb.move.rawValue)
                    coords += [x, y]
                case .line(let x, let y):
                    verbs.append(GeometryVerb.line.rawValue)
                    coords += [x, y]
                case .cubic(let x1, let y1, let x2, let y2, let x, let y):
                    verbs.append(GeometryVerb.cubic.rawValue)
                    coords += [x1, y1, x2, y2, x, y]
                }
            }
            parts.append(
                GeometryPart(
                    element: UInt32(index), version: 1, kind: part.kind, color: part.color, opacity: part.opacity,
                    width: part.width, verbStart: UInt32(verbStart), verbCount: UInt32(verbs.count - verbStart),
                    coordStart: UInt32(coordStart), dashOn: 0, dashOff: 0, fillRule: part.rule.rawValue))
        }
    }

    convenience init(_ part: HandBuiltPart...) {
        self.init(part)
    }

    func generation() -> UInt64 { 1 }

    func withBuffers<R>(_ body: (GeometryBuffers) throws -> R) rethrows -> R {
        try parts.withUnsafeBufferPointer { parts in
            try verbs.withUnsafeBufferPointer { verbs in
                try coords.withUnsafeBufferPointer { coords in
                    try body(GeometryBuffers(parts: parts, verbs: verbs, coords: coords))
                }
            }
        }
    }
}
