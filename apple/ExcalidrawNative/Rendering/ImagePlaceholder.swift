import Foundation

/// What is drawn in an image's box while there are no pixels for its file:
/// Excalidraw's placeholder (`drawImagePlaceholder`), a light grey box with a
/// picture icon, or a crossed-out one if loading the file failed. Both are
/// described as filled paths, so they stay sharp at any zoom.
enum ImagePlaceholder {
    /// A filled path in the box's own frame: origin at its top-left corner,
    /// unrotated and unmirrored.
    struct Fill {
        var color: UInt32
        var path: SVGPathData
    }

    /// Excalidraw's light and dark placeholder backgrounds. The icon is a
    /// picture of its own there, the same in both.
    private static func backgroundColor(dark: Bool) -> UInt32 { dark ? 0x2e2e_2eff : 0xe7e7_e7ff }
    private static let iconColor: UInt32 = 0x8888_88ff
    /// The icon is at most this large, in Scene units.
    private static let largestIcon: Float = 100

    /// The fills for a `width` x `height` box, background first.
    static func fills(width: Float, height: Float, failed: Bool, dark: Bool = false) -> [Fill] {
        let backgroundColor = backgroundColor(dark: dark)
        let box = SVGPathData(
            verbs: [GeometryVerb.move, .line, .line, .line].map(\.rawValue),
            coords: [0, 0, width, 0, width, height, 0, height])
        let shorterSide = min(width, height)
        let size = min(shorterSide * 0.4, largestIcon)
        guard size > 0 else { return [Fill(color: backgroundColor, path: box)] }
        let (artwork, artworkSize) = failed ? (errorIcon, Float(668)) : (icon, Float(512))
        let scale = size / artworkSize
        let origin = SIMD2((width - size) / 2, (height - size) / 2)
        return [Fill(color: backgroundColor, path: box)]
            + artwork.map {
                Fill(
                    color: iconColor,
                    path: $0.path.transformed(scale: scale * $0.scale, translation: origin + scale * $0.translation))
            }
    }

    /// A path of the artwork with the transform the SVG gives it.
    private struct Piece {
        var path: SVGPathData
        var scale: Float = 1
        var translation = SIMD2<Float>.zero
    }

    // The two icons are Excalidraw's own (Font Awesome "image", and the same
    // with a "ban" sign); the numbers are copied from its SVG sources.

    private static let icon = [
        Piece(
            path: SVGPathData(
                """
                M464 448H48c-26.51 0-48-21.49-48-48V112c0-26.51 21.49-48 48-48h416c26.51 0 48 21.49 48 48v288\
                c0 26.51-21.49 48-48 48zM112 120c-30.928 0-56 25.072-56 56s25.072 56 56 56 56-25.072 56-56\
                -25.072-56-56-56zM64 384h384V272l-87.515-87.515c-4.686-4.686-12.284-4.686-16.971 0L208 320\
                l-55.515-55.515c-4.686-4.686-12.284-4.686-16.971 0L64 336v48z
                """))
    ]

    private static let errorIcon = [
        Piece(
            path: SVGPathData(
                """
                M464 448H48c-26.51 0-48-21.49-48-48V112c0-26.51 21.49-48 48-48h416c26.51 0 48 21.49 48 48v288\
                c0 26.51-21.49 48-48 48ZM112 120c-30.928 0-56 25.072-56 56s25.072 56 56 56 56-25.072 56-56\
                -25.072-56-56-56ZM64 384h384V272l-87.515-87.515c-4.686-4.686-12.284-4.686-16.971 0L208 320\
                l-55.515-55.515c-4.686-4.686-12.284-4.686-16.971 0L64 336v48Z
                """), scale: 0.81709, translation: [124.825, 145.825]),
        Piece(
            path: SVGPathData(
                """
                M256 8C119.034 8 8 119.033 8 256c0 136.967 111.034 248 248 248s248-111.034 248-248\
                S392.967 8 256 8Zm130.108 117.892c65.448 65.448 70 165.481 20.677 235.637L150.47 105.216\
                c70.204-49.356 170.226-44.735 235.638 20.676ZM125.892 386.108c-65.448-65.448-70-165.481\
                -20.677-235.637L361.53 406.784c-70.203 49.356-170.226 44.736-235.638-20.676Z
                """), scale: 0.30366, translation: [506.822, 60.065]),
    ]
}

extension SVGPathData {
    /// The path scaled by `scale` and then moved by `translation`.
    func transformed(scale: Float, translation: SIMD2<Float>) -> SVGPathData {
        var result = self
        for index in stride(from: 0, to: coords.count, by: 2) {
            result.coords[index] = coords[index] * scale + translation.x
            result.coords[index + 1] = coords[index + 1] * scale + translation.y
        }
        return result
    }
}

extension GeometryImage {
    /// Where the point `local` of the image's box (origin at its top-left
    /// corner) lies in the Scene: mirrored and turned about the box's centre
    /// as the record says.
    func scenePoint(_ local: SIMD2<Float>) -> SIMD2<Float> {
        let centre = SIMD2(width, height) / 2
        let offset = (local - centre) * SIMD2(scaleX, scaleY)
        let (sin, cos) = (sinf(angle), cosf(angle))
        return SIMD2(x, y) + centre + SIMD2(offset.x * cos - offset.y * sin, offset.x * sin + offset.y * cos)
    }
}
