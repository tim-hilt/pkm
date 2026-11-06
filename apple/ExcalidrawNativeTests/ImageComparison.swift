@testable import ExcalidrawNative

/// Why a rendered image does not match its reference.
struct ImageMismatch {
    /// One line for the test failure message.
    let summary: String
    /// Pixels beyond the tolerance; 0 if the sizes differ and none were compared.
    let differingPixels: Int
    /// The reference with every differing pixel painted red; nil if the sizes differ.
    let diff: RenderedImage?
}

/// How far a render may stray from its reference and still match.
struct ImageTolerance {
    /// Largest per-channel difference (0...255) of a pixel that still counts as
    /// equal. Covers anti-aliasing coverage that varies slightly between GPUs;
    /// on one machine renders are identical. Low enough that a 10 % change of
    /// opacity or a colour one shade off fails.
    var channel = 8
    /// Share of pixels allowed to differ by more than `channel`, e.g. MSAA samples
    /// landing differently on a few edge pixels: 32 pixels of a 400 x 400
    /// render. A moved, missing or recoloured element changes more than this.
    var pixelFraction = 0.0002

    static let standard = ImageTolerance()
}

enum ImageComparison {
    /// Nil if `actual` matches `expected` within `tolerance`.
    static func mismatch(
        between actual: RenderedImage, and expected: RenderedImage, tolerance: ImageTolerance = .standard
    ) -> ImageMismatch? {
        guard actual.width == expected.width, actual.height == expected.height else {
            return ImageMismatch(
                summary: "size is \(actual.width)x\(actual.height), reference is \(expected.width)x\(expected.height)",
                differingPixels: 0, diff: nil)
        }

        let bytesPerPixel = RenderedImage.bytesPerPixel
        var diff = expected.bgra
        var differing = 0
        for start in stride(from: 0, to: diff.count, by: bytesPerPixel) {
            var largest = 0
            for channel in 0..<bytesPerPixel {
                largest = max(largest, abs(Int(actual.bgra[start + channel]) - Int(expected.bgra[start + channel])))
            }
            if largest > tolerance.channel {
                differing += 1
                diff[start...(start + 3)] = [0, 0, 255, 255]
            }
        }

        let allowed = Int(tolerance.pixelFraction * Double(expected.width * expected.height))
        guard differing > allowed else { return nil }
        return ImageMismatch(
            summary: "\(differing) pixels differ by more than \(tolerance.channel)/255 (at most \(allowed) allowed)",
            differingPixels: differing,
            diff: RenderedImage(width: expected.width, height: expected.height, bgra: diff))
    }
}
