/// Swift view of the core's geometry output (ADR-0007). The format is
/// documented in the core's `geometry` module; this file mirrors it.

/// Mirror of the core's `Part` record, read straight out of the core's memory.
/// Field order and sizes must match; `withBuffers` checks the record size.
struct GeometryPart {
    /// Constant-width outline with round caps and joins.
    static let strokeKind: UInt32 = 0
    /// The area enclosed by the path, whose subpaths are implicitly closed;
    /// `fillRule` says what counts as enclosed.
    static let fillKind: UInt32 = 1

    /// Core-assigned Element handle; with `version` the cache key.
    var element: UInt32
    var version: UInt32
    var kind: UInt32
    /// `0xRRGGBBAA`.
    var color: UInt32
    /// Element opacity, 0...1.
    var opacity: Float
    /// Line width in Scene units; 0 for fills.
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
    /// A `GeometryFillRule` number; 0 for strokes.
    var fillRule: UInt32 = 0

    /// How a fill treats overlapping stretches of its path.
    var rule: GeometryFillRule? { GeometryFillRule(rawValue: fillRule) }

    /// The dash pattern, if the part is dashed. Like a canvas' `setLineDash`
    /// it starts over at every `move`, and the round caps come on top of the
    /// dashes' length.
    var dash: (on: Float, off: Float)? {
        dashOn > 0 && dashOff > 0 ? (dashOn, dashOff) : nil
    }
}

/// Mirror of the core's `TextLine` record: one line of text to draw, set in
/// Excalifont. Field order and sizes must match.
struct GeometryTextLine {
    /// Core-assigned Element handle; with `version` the cache key.
    var element: UInt32
    var version: UInt32
    /// Number of parts before this line in z-order: draw the line after the
    /// parts before this index and before the part at it.
    var partIndex: UInt32
    /// `0xRRGGBBAA`.
    var color: UInt32
    /// Element opacity, 0...1.
    var opacity: Float
    var fontSize: Float
    /// The Element's font family. Every family is drawn in Excalifont.
    var fontFamily: UInt32
    /// A `GeometryTextAlign` number.
    var align: UInt32
    /// Where the line is anchored on its baseline, in Scene units, already
    /// turned by `angle`.
    var x: Float
    var y: Float
    /// Clockwise rotation in radians about `(x, y)`.
    var angle: Float
    /// Range of the line's UTF-8 bytes in `GeometryBuffers.text`.
    var textStart: UInt32
    var textLength: UInt32

    var alignment: GeometryTextAlign { GeometryTextAlign(rawValue: align) ?? .left }
}

/// Mirror of the core's `Image` record: one image to draw. Field order and
/// sizes must match. The box is `width` x `height` with its top-left corner at
/// `(x, y)`; draw it turned by `angle` about its centre, then mirrored about
/// its centre by `scaleX`/`scaleY` (each 1 or -1), with the part `(u0, v0)` to
/// `(u1, v1)` of the file's pixels (as fractions of its size) filling the box.
struct GeometryImage {
    /// Core-assigned Element handle; with `version` the cache key.
    var element: UInt32
    var version: UInt32
    /// Number of parts before this image in z-order: draw the image after the
    /// parts before this index and before the part at it.
    var partIndex: UInt32
    /// Range of the file id's UTF-8 bytes in `GeometryBuffers.fileIDs`; empty
    /// for an image without a file.
    var fileIDStart: UInt32
    var fileIDLength: UInt32
    /// A `GeometryImageStatus` number.
    var status: UInt32
    /// Element opacity, 0...1.
    var opacity: Float
    var x: Float
    var y: Float
    var width: Float
    var height: Float
    /// Clockwise rotation in radians about the box's centre.
    var angle: Float
    var scaleX: Float
    var scaleY: Float
    var u0: Float
    var v0: Float
    var u1: Float
    var v1: Float
    /// Radius of the box's rounded corners in Scene units; 0 for sharp ones.
    var cornerRadius: Float

    var imageStatus: GeometryImageStatus { GeometryImageStatus(rawValue: status) ?? .pending }
}

/// How far an image's file is known to have got, as in the core's `ImageStatus`.
enum GeometryImageStatus: UInt32 {
    case pending = 0
    case saved = 1
    /// Loading the file failed.
    case error = 2
}

/// Where a line of text sits relative to its anchor, as in the core's `TextAlign`.
enum GeometryTextAlign: UInt32 {
    /// The line starts at the anchor.
    case left = 0
    /// The line is centred on the anchor.
    case center = 1
    /// The line ends at the anchor.
    case right = 2
}

/// Which points a fill covers where its path crosses itself, as in the core's
/// `FillRule`.
enum GeometryFillRule: UInt32 {
    /// Inside where the path winds around at all: overlaps stay filled.
    case nonZero = 0
    /// Inside where a ray crosses the path an odd number of times: overlaps are holes.
    case evenOdd = 1
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

/// The core's geometry buffers, borrowed. Only valid inside the
/// `Geometry.withBuffers` call that produced them.
struct GeometryBuffers {
    let parts: UnsafeBufferPointer<GeometryPart>
    let verbs: UnsafeBufferPointer<UInt8>
    let coords: UnsafeBufferPointer<Float>
    /// Lines of text, interleaved with `parts` in z-order by `partIndex`.
    var textLines = UnsafeBufferPointer<GeometryTextLine>(start: nil, count: 0)
    /// The characters of all lines, as UTF-8.
    var text = UnsafeBufferPointer<UInt8>(start: nil, count: 0)
    /// Images, interleaved with `parts` in z-order by `partIndex`.
    var images = UnsafeBufferPointer<GeometryImage>(start: nil, count: 0)
    /// The file ids of all images, as UTF-8.
    var fileIDs = UnsafeBufferPointer<UInt8>(start: nil, count: 0)

    /// The id of the file `image` shows; empty if it has none.
    func fileID(of image: GeometryImage) -> String {
        let bytes = fileIDs[Int(image.fileIDStart)..<Int(image.fileIDStart + image.fileIDLength)]
        return String(decoding: bytes, as: UTF8.self)
    }

    /// The characters of `line`.
    func string(of line: GeometryTextLine) -> String {
        let bytes = text[Int(line.textStart)..<Int(line.textStart + line.textLength)]
        return String(decoding: bytes, as: UTF8.self)
    }

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

/// What one Element contributes to the geometry. An Element is drawn by a
/// run of parts, by a run of lines of text or by one image, never a mix.
struct ElementGeometry {
    enum Content {
        /// Indices into `GeometryBuffers.parts`.
        case parts(Range<Int>)
        /// Indices into `GeometryBuffers.textLines`.
        case textLines(Range<Int>)
        /// Index into `GeometryBuffers.images`.
        case image(Int)
    }

    var element: UInt32
    var version: UInt32
    var content: Content
}

extension GeometryBuffers {
    /// Every Element's share of the buffers, in z-order (bottom first).
    func elementsInZOrder() -> some Sequence<ElementGeometry> {
        sequence(state: (part: 0, line: 0, image: 0)) { next -> ElementGeometry? in
            let (part, line, image) = next
            // Lines and images come before the part their `partIndex` names.
            // Lines and images due at the same part belong to Elements
            // without parts between them; the lower handle is lower in z-order.
            let lineDue = line < textLines.count && Int(textLines[line].partIndex) <= part
            let imageDue = image < images.count && Int(images[image].partIndex) <= part
            if imageDue && (!lineDue || images[image].element < textLines[line].element) {
                next.image += 1
                let current = images[image]
                return ElementGeometry(element: current.element, version: current.version, content: .image(image))
            }
            if lineDue {
                let first = textLines[line]
                var end = line + 1
                while end < textLines.count, textLines[end].element == first.element { end += 1 }
                next.line = end
                return ElementGeometry(element: first.element, version: first.version, content: .textLines(line..<end))
            }
            guard part < parts.count else { return nil }
            let first = parts[part]
            var end = part + 1
            while end < parts.count, parts[end].element == first.element { end += 1 }
            next.part = end
            return ElementGeometry(element: first.element, version: first.version, content: .parts(part..<end))
        }
    }
}

/// Geometry the renderer can draw: the core's `Geometry`, or hand-built
/// geometry in tests.
protocol GeometrySource: AnyObject {
    /// Tells whether cached per-Element results are still valid, see the
    /// core's `Geometry::generation`.
    func generation() -> UInt64
    /// Runs `body` with the buffers, which must not escape it.
    func withBuffers<R>(_ body: (GeometryBuffers) throws -> R) rethrows -> R
}

extension Geometry: GeometrySource {}

extension Geometry {
    /// Runs `body` with the core's buffers, read in place without copying.
    /// The buffers must not escape `body`: they belong to this object.
    func withBuffers<R>(_ body: (GeometryBuffers) throws -> R) rethrows -> R {
        precondition(
            MemoryLayout<GeometryPart>.stride == Int(partStride()),
            "GeometryPart no longer mirrors the core's Part record")
        precondition(
            MemoryLayout<GeometryTextLine>.stride == Int(textLineStride()),
            "GeometryTextLine no longer mirrors the core's TextLine record")
        precondition(
            MemoryLayout<GeometryImage>.stride == Int(imageStride()),
            "GeometryImage no longer mirrors the core's Image record")
        return try withExtendedLifetime(self) {
            try body(
                GeometryBuffers(
                    parts: buffer(at: partsAddress(), count: partCount()),
                    verbs: buffer(at: verbsAddress(), count: verbCount()),
                    coords: buffer(at: coordsAddress(), count: coordCount()),
                    textLines: buffer(at: textLinesAddress(), count: textLineCount()),
                    text: buffer(at: textAddress(), count: textByteCount()),
                    images: buffer(at: imagesAddress(), count: imageCount()),
                    fileIDs: buffer(at: fileIdsAddress(), count: fileIdByteCount())))
        }
    }
}

private func buffer<T>(at address: UInt64, count: UInt64) -> UnsafeBufferPointer<T> {
    UnsafeBufferPointer(start: UnsafePointer<T>(bitPattern: UInt(address)), count: Int(count))
}
