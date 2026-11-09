/// One straight stroke segment in Scene coordinates: `(x0, y0, x1, y1)`.
typealias Segment = SIMD4<Float>

/// Turns a stroked part's paths into straight segments: curves are flattened
/// (see `PathFlattener`) and dashes cut along the flattened path.
enum StrokeFlattener {
    /// The segments of `part`, in drawing order.
    ///
    /// - Parameter tolerance: See `PathFlattener.flatten`.
    ///
    /// A `move` starts a new subpath without producing a segment; zero-length
    /// `line`s are kept, they draw a round dot. For a dashed part only the
    /// drawn stretches are returned, split at the path's vertices.
    static func segments(of part: GeometryPart, in buffers: GeometryBuffers, tolerance: Float) -> [Segment] {
        var output = SegmentWriter(dash: part.dash)
        PathFlattener.flatten(part, in: buffers, tolerance: tolerance, into: &output)
        return output.segments
    }

    /// Collects segments, cutting them into dashes if the stroke is dashed.
    private struct SegmentWriter: PathSink {
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
        mutating func beginSubpath(at point: SIMD2<Float>) {
            drawing = true
            remaining = dash?.on ?? 0
        }

        mutating func addLine(from start: SIMD2<Float>, to end: SIMD2<Float>) {
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
}

extension SIMD4<Float> {
    fileprivate init(_ start: SIMD2<Float>, _ end: SIMD2<Float>) {
        self.init(start.x, start.y, end.x, end.y)
    }
}
