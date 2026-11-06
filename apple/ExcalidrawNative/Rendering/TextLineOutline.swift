import Foundation

/// A line of text as one filled path in Scene coordinates: the outlines of
/// its glyphs, scaled, aligned and turned as the core's `TextLine` says.
/// Drawn with the nonzero rule like any glyph outline.
enum TextLineOutline {
    /// `verbs` and `coords` in the format of the core's geometry.
    struct Path {
        var verbs: [UInt8] = []
        var coords: [Float] = []
    }

    /// The path for `line`, whose characters are `text`.
    static func path(of line: GeometryTextLine, text: String, font: Excalifont) -> Path {
        let layout = font.layout(of: text)
        let scale = line.fontSize / Float(Excalifont.designSize)
        let shift: Float =
            switch line.alignment {
            case .left: 0
            case .center: -layout.width / 2
            case .right: -layout.width
            }
        let (sin, cos) = (sinf(line.angle), cosf(line.angle))

        var path = Path()
        for glyph in layout.glyphs {
            path.verbs += glyph.outline.verbs
            var coords = glyph.outline.coords[...]
            while let x = coords.popFirst(), let y = coords.popFirst() {
                let (local, down) = ((glyph.x + shift + x) * scale, y * scale)
                path.coords.append(line.x + local * cos - down * sin)
                path.coords.append(line.y + local * sin + down * cos)
            }
        }
        return path
    }
}
