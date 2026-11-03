/// The path of an SVG `d` attribute as ops in the format of the core's
/// geometry (`GeometryVerb`), so the renderer can fill it like any other path.
///
/// Understands what icon paths are made of: move, line, horizontal and
/// vertical line, cubic Bézier (also the smooth form) and close, absolute or
/// relative. Quadratic curves and arcs are not supported: a path using them
/// parses as far as it can.
struct SVGPathData: Equatable {
    var verbs: [UInt8] = []
    var coords: [Float] = []

    init(verbs: [UInt8], coords: [Float]) {
        self.verbs = verbs
        self.coords = coords
    }

    /// Parses the value of a `d` attribute.
    init(_ data: String) {
        var reader = Reader(data)
        var current = SIMD2<Float>.zero
        var subpathStart = SIMD2<Float>.zero
        /// Second control point of the previous cubic, for the smooth form.
        var previousControl: SIMD2<Float>?
        var command: Character = "M"
        /// After a close the next subpath begins at the old one's start.
        var needsMove = false

        func move(to point: SIMD2<Float>) {
            verbs.append(GeometryVerb.move.rawValue)
            coords += [point.x, point.y]
        }

        while true {
            reader.skipSeparators()
            if let next = reader.peekCommand() {
                command = next
                reader.advance()
                if command == "Z" || command == "z" {
                    current = subpathStart
                    previousControl = nil
                    needsMove = true
                    continue
                }
            } else if reader.atEnd {
                break
            }
            // A command letter may be followed by several argument sets; a
            // move's further sets are lines.
            let relative = command.isLowercase
            let origin = relative ? current : .zero
            if needsMove && command.uppercased() != "M" { move(to: current) }
            needsMove = false

            switch command.uppercased() {
            case "M":
                guard let point = reader.point(relativeTo: origin) else { return }
                move(to: point)
                current = point
                subpathStart = point
                previousControl = nil
                command = relative ? "l" : "L"
            case "L":
                guard let point = reader.point(relativeTo: origin) else { return }
                line(to: point)
                current = point
                previousControl = nil
            case "H":
                guard let x = reader.number() else { return }
                current.x = relative ? current.x + x : x
                line(to: current)
                previousControl = nil
            case "V":
                guard let y = reader.number() else { return }
                current.y = relative ? current.y + y : y
                line(to: current)
                previousControl = nil
            case "C", "S":
                var first: SIMD2<Float>
                if command.uppercased() == "S" {
                    // The reflection of the previous control point.
                    first = previousControl.map { 2 * current - $0 } ?? current
                } else {
                    guard let point = reader.point(relativeTo: origin) else { return }
                    first = point
                }
                guard let second = reader.point(relativeTo: origin),
                    let end = reader.point(relativeTo: origin)
                else { return }
                verbs.append(GeometryVerb.cubic.rawValue)
                coords += [first.x, first.y, second.x, second.y, end.x, end.y]
                current = end
                previousControl = second
            default:
                return
            }
        }
    }

    private mutating func line(to point: SIMD2<Float>) {
        verbs.append(GeometryVerb.line.rawValue)
        coords += [point.x, point.y]
    }

    /// Cursor over the text of the attribute.
    private struct Reader {
        private let bytes: [UInt8]
        private var position = 0

        init(_ text: String) { bytes = Array(text.utf8) }

        var atEnd: Bool { position >= bytes.count }

        mutating func skipSeparators() {
            while position < bytes.count, bytes[position] == UInt8(ascii: ",") || bytes[position] <= UInt8(ascii: " ") {
                position += 1
            }
        }

        func peekCommand() -> Character? {
            guard position < bytes.count else { return nil }
            let character = Character(UnicodeScalar(bytes[position]))
            return character.isLetter ? character : nil
        }

        mutating func advance() { position += 1 }

        mutating func point(relativeTo origin: SIMD2<Float>) -> SIMD2<Float>? {
            guard let x = number(), let y = number() else { return nil }
            return origin + SIMD2(x, y)
        }

        /// `-?digits[.digits]`, as it appears in path data, where a sign or a
        /// second dot starts the next number.
        mutating func number() -> Float? {
            skipSeparators()
            let start = position
            if position < bytes.count, bytes[position] == UInt8(ascii: "-") || bytes[position] == UInt8(ascii: "+") {
                position += 1
            }
            var seenDot = false
            while position < bytes.count {
                let byte = bytes[position]
                if byte >= UInt8(ascii: "0") && byte <= UInt8(ascii: "9") {
                    position += 1
                } else if byte == UInt8(ascii: ".") && !seenDot {
                    seenDot = true
                    position += 1
                } else {
                    break
                }
            }
            return Float(String(decoding: bytes[start..<position], as: UTF8.self))
        }
    }
}
