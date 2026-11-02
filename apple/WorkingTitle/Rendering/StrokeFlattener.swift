/// One straight stroke segment in Scene coordinates: `(x0, y0, x1, y1)`.
typealias Segment = SIMD4<Float>

/// Turns a stroked part's paths into straight segments. Curves are subdivided
/// on the client (ADR-0008) because only the client knows how large a Scene
/// unit is on screen; dashes are cut here too, along the flattened path.
enum StrokeFlattener {
    /// The segments of `part`, in drawing order.
    ///
    /// - Parameter tolerance: Largest allowed distance, in Scene units,
    ///   between a curve and the segments replacing it. Callers derive it from
    ///   the zoom so that the error stays a fraction of a device pixel.
    ///
    /// A `move` starts a new subpath without producing a segment; zero-length
    /// `line`s are kept, they draw a round dot. For a dashed part only the
    /// drawn stretches are returned, split at the path's vertices.
    static func segments(of part: GeometryPart, in buffers: GeometryBuffers, tolerance: Float) -> [Segment] {
        let coords = buffers.coords(of: part)
        var next = 0
        var current = SIMD2<Float>.zero
        var output = SegmentWriter(dash: part.dash)

        func point() -> SIMD2<Float> {
            defer { next += 2 }
            return SIMD2(coords[next], coords[next + 1])
        }

        for byte in buffers.verbs(of: part) {
            switch GeometryVerb(rawValue: byte) {
            case .move:
                current = point()
                output.beginSubpath()
            case .line:
                let end = point()
                output.add(from: current, to: end)
                current = end
            case .cubic:
                let (c1, c2, end) = (point(), point(), point())
                let steps = stepCount(current, c1, c2, end, tolerance: tolerance)
                var previous = current
                for step in 1...steps {
                    let t = Float(step) / Float(steps)
                    // Land exactly on the end point instead of on rounding noise.
                    let p = step == steps ? end : cubic(current, c1, c2, end, at: t)
                    output.add(from: previous, to: p)
                    previous = p
                }
                current = end
            case nil:
                preconditionFailure("unknown verb \(byte)")
            }
        }
        return output.segments
    }

    /// Collects segments, cutting them into dashes if the stroke is dashed.
    private struct SegmentWriter {
        private(set) var segments: [Segment] = []
        private let dash: (on: Float, off: Float)?
        /// Whether the pattern is in a drawn stretch, and how much of that
        /// stretch or gap is left.
        private var drawing = true
        private var remaining: Float

        init(dash: (on: Float, off: Float)?) {
            self.dash = dash
            remaining = dash?.on ?? 0
        }

        /// The pattern starts over with every subpath.
        mutating func beginSubpath() {
            drawing = true
            remaining = dash?.on ?? 0
        }

        mutating func add(from start: SIMD2<Float>, to end: SIMD2<Float>) {
            guard let dash else {
                segments.append(Segment(start, end))
                return
            }
            let length = ((end - start) * (end - start)).sum().squareRoot()
            guard length > 0 else { return }
            let direction = (end - start) / length
            var travelled: Float = 0
            while travelled < length {
                let left = length - travelled
                let step = min(left, remaining)
                // Land exactly on the end when the stretch reaches it.
                let reachesEnd = step == left
                if drawing {
                    let from = start + direction * travelled
                    segments.append(Segment(from, reachesEnd ? end : start + direction * (travelled + step)))
                }
                travelled = reachesEnd ? length : travelled + step
                remaining -= step
                if remaining <= 0 {
                    drawing.toggle()
                    remaining = drawing ? dash.on : dash.off
                }
            }
        }
    }

    /// Number of chords that keep a cubic within `tolerance`, from the bound
    /// on the second differences of its control polygon (Wang's formula for
    /// degree 3: n = sqrt(6 * max|second difference| / (8 * tolerance))).
    private static func stepCount(
        _ p0: SIMD2<Float>, _ p1: SIMD2<Float>, _ p2: SIMD2<Float>, _ p3: SIMD2<Float>, tolerance: Float
    ) -> Int {
        let d1 = p0 - 2 * p1 + p2
        let d2 = p1 - 2 * p2 + p3
        let longest = max((d1 * d1).sum(), (d2 * d2).sum()).squareRoot()
        let steps = (0.75 * longest / max(tolerance, .leastNormalMagnitude)).squareRoot().rounded(.up)
        return steps.isFinite ? min(max(Int(steps), 1), 1_000) : 1
    }

    private static func cubic(
        _ p0: SIMD2<Float>, _ p1: SIMD2<Float>, _ p2: SIMD2<Float>, _ p3: SIMD2<Float>, at t: Float
    ) -> SIMD2<Float> {
        let u = 1 - t
        return u * u * u * p0 + 3 * u * u * t * p1 + 3 * u * t * t * p2 + t * t * t * p3
    }
}

extension SIMD4<Float> {
    fileprivate init(_ start: SIMD2<Float>, _ end: SIMD2<Float>) {
        self.init(start.x, start.y, end.x, end.y)
    }
}
