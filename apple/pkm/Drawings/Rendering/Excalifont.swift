import CoreGraphics
import CoreText
import Foundation

/// The outline of one glyph in the font's design units (1000 per em), y
/// pointing down like the Scene's. `verbs` and `coords` are in the format of
/// the core's geometry (`GeometryVerb`); every contour is closed implicitly,
/// as fills treat their paths.
struct GlyphOutline {
    var verbs: [UInt8] = []
    var coords: [Float] = []
}

/// A line of text set in Excalifont at the design size: each glyph's outline
/// at its position, and the line's advance width.
struct LineLayout {
    struct Glyph {
        var outline: GlyphOutline
        /// Where the glyph's origin is on the baseline.
        var x: Float
    }
    var glyphs: [Glyph] = []
    var width: Float = 0
}

/// Excalifont, the only bundled font. It measures text for the core (the
/// `TextMeasurer` port) and hands the renderer glyph outlines, cached per glyph, so
/// that text is drawn as filled paths that stay sharp at any zoom
/// (ADR-0008).
///
/// Every font family of a Drawing is drawn in Excalifont; characters the
/// font lacks come from the system's fallback fonts.
final class Excalifont: TextMeasurer, @unchecked Sendable {
    /// The design size: units per em of the font, and the size every line is
    /// set at so that advances and outlines are in design units.
    static let designSize: CGFloat = 1000

    private final class BundleToken {}

    /// The bundled font, nil if the app lacks it.
    static let shared = Excalifont(bundle: Bundle(for: BundleToken.self))

    private struct GlyphKey: Hashable {
        var font: String
        var glyph: CGGlyph
    }

    private let font: CTFont
    private let url: URL
    /// Guards `glyphs`; the core may measure from another thread than the renderer draws on.
    private let lock = NSLock()
    private var glyphs: [GlyphKey: GlyphOutline] = [:]

    init?(bundle: Bundle) {
        guard let url = bundle.url(forResource: "Excalifont-Regular", withExtension: "ttf"),
            let provider = CGDataProvider(url: url as CFURL),
            let graphicsFont = CGFont(provider)
        else { return nil }
        font = CTFontCreateWithGraphicsFont(graphicsFont, Self.designSize, nil, nil)
        self.url = url
    }

    /// The font's PostScript name once the font is registered for this process
    /// (see `registerForTextViews`).
    var postScriptName: String { CTFontCopyPostScriptName(font) as String }

    /// Makes the font known by name to this process, so that the platform's text
    /// views can set text in it (they cannot be given a `CTFont`). Registering
    /// again is harmless. Returns whether the font can be used by name.
    @discardableResult
    func registerForTextViews() -> Bool {
        var error: Unmanaged<CFError>?
        if CTFontManagerRegisterFontsForURL(url as CFURL, .process, &error) { return true }
        // Already registered is as good as registered.
        let code = error.map { CFErrorGetCode($0.takeRetainedValue()) }
        return code == CFIndex(CTFontManagerError.alreadyRegistered.rawValue)
    }

    /// Number of distinct glyph outlines made so far; a glyph is made once,
    /// however often and wherever it is drawn.
    var cachedGlyphCount: Int {
        lock.withLock { glyphs.count }
    }

    // MARK: TextMeasurer

    func lineWidth(text: String, fontSize: Double) -> Double {
        let line = ctLine(text)
        return CTLineGetTypographicBounds(line, nil, nil, nil) * fontSize / Self.designSize
    }

    // MARK: Outlines

    /// `text`, a single line, laid out at the design size.
    func layout(of text: String) -> LineLayout {
        let line = ctLine(text)
        var layout = LineLayout(width: Float(CTLineGetTypographicBounds(line, nil, nil, nil)))
        for run in CTLineGetGlyphRuns(line) as! [CTRun] {
            let count = CTRunGetGlyphCount(run)
            guard count > 0 else { continue }
            let attributes = CTRunGetAttributes(run) as NSDictionary
            let runFont = attributes[kCTFontAttributeName] as! CTFont
            var ids = [CGGlyph](repeating: 0, count: count)
            var positions = [CGPoint](repeating: .zero, count: count)
            CTRunGetGlyphs(run, CFRange(location: 0, length: count), &ids)
            CTRunGetPositions(run, CFRange(location: 0, length: count), &positions)
            for (id, position) in zip(ids, positions) {
                let outline = outline(of: id, in: runFont)
                guard !outline.verbs.isEmpty else { continue }
                layout.glyphs.append(.init(outline: outline, x: Float(position.x)))
            }
        }
        return layout
    }

    private func ctLine(_ text: String) -> CTLine {
        let attributed = NSAttributedString(
            string: text, attributes: [kCTFontAttributeName as NSAttributedString.Key: font])
        return CTLineCreateWithAttributedString(attributed)
    }

    private func outline(of glyph: CGGlyph, in font: CTFont) -> GlyphOutline {
        let key = GlyphKey(font: CTFontCopyPostScriptName(font) as String, glyph: glyph)
        if let cached = lock.withLock({ glyphs[key] }) { return cached }
        let outline = CTFontCreatePathForGlyph(font, glyph, nil).map(Self.outline(of:)) ?? GlyphOutline()
        lock.withLock { glyphs[key] = outline }
        return outline
    }

    /// `path` in the core's path format, flipped to y pointing down.
    private static func outline(of path: CGPath) -> GlyphOutline {
        var outline = GlyphOutline()
        var current = CGPoint.zero
        func add(_ verb: GeometryVerb, _ points: CGPoint...) {
            outline.verbs.append(verb.rawValue)
            for point in points { outline.coords += [Float(point.x), Float(-point.y)] }
        }
        path.applyWithBlock { element in
            let points = element.pointee.points
            switch element.pointee.type {
            case .moveToPoint:
                add(.move, points[0])
                current = points[0]
            case .addLineToPoint:
                add(.line, points[0])
                current = points[0]
            case .addQuadCurveToPoint:
                // TrueType outlines are quadratic; raise them to cubics.
                let (control, end) = (points[0], points[1])
                add(
                    .cubic,
                    CGPoint(
                        x: current.x + 2 / 3 * (control.x - current.x), y: current.y + 2 / 3 * (control.y - current.y)),
                    CGPoint(x: end.x + 2 / 3 * (control.x - end.x), y: end.y + 2 / 3 * (control.y - end.y)), end)
                current = end
            case .addCurveToPoint:
                add(.cubic, points[0], points[1], points[2])
                current = points[2]
            case .closeSubpath:
                break
            @unknown default:
                break
            }
        }
        return outline
    }
}

extension Editor {
    /// An editor that measures text with the bundled Excalifont (or, should the
    /// app lack it, without a measurer, so that labels keep the lines saved in
    /// the file).
    static func withExcalifont() -> Editor {
        let editor = Editor()
        if let font = Excalifont.shared { editor.setTextMeasurer(measurer: font) }
        return editor
    }
}
