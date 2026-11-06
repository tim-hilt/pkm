import Foundation
import Testing

@testable import ExcalidrawNative

/// The comparison is the test oracle for every reference image, so its own
/// behaviour is pinned down with small synthetic images.
@Suite struct ImageComparisonTests {
    private static let size = 100

    /// White paper with a grey vertical line `thickness` pixels wide at column `x`.
    private func image(lineAt x: Int? = nil, thickness: Int = 1, grey: UInt8 = 0, size: Int = size) -> RenderedImage {
        var bgra = [UInt8](repeating: 255, count: size * size * RenderedImage.bytesPerPixel)
        if let x {
            for row in 0..<size {
                for column in x..<(x + thickness) {
                    let pixel = (row * size + column) * RenderedImage.bytesPerPixel
                    bgra[pixel...(pixel + 2)] = [grey, grey, grey]
                }
            }
        }
        return RenderedImage(width: size, height: size, bgra: bgra)
    }

    @Test func identicalImagesMatch() {
        #expect(ImageComparison.mismatch(between: image(lineAt: 40), and: image(lineAt: 40)) == nil)
    }

    @Test func faintAntiAliasingDifferencesAreTolerated() {
        // Every pixel of the line is a few levels off, as after a GPU change.
        let expected = image(lineAt: 40, grey: 100)
        let actual = image(lineAt: 40, grey: 105)
        #expect(ImageComparison.mismatch(between: actual, and: expected) == nil)
    }

    @Test func aSlightlyOtherColourIsAMismatch() {
        // Ink (30) at 90 % opacity over paper is 52.
        #expect(ImageComparison.mismatch(between: image(lineAt: 40, grey: 52), and: image(lineAt: 40, grey: 30)) != nil)
    }

    @Test func aFewStrayPixelsAreTolerated() {
        // A handful of edge pixels flip, e.g. a different MSAA sample position.
        let actual = (0..<2).reduce(image(lineAt: 40)) { $0.with(pixel: (41, $1), grey: 0) }
        #expect(ImageComparison.mismatch(between: actual, and: image(lineAt: 40)) == nil)
    }

    @Test func aMovedLineIsAMismatch() throws {
        let mismatch = try #require(ImageComparison.mismatch(between: image(lineAt: 41), and: image(lineAt: 40)))
        #expect(mismatch.differingPixels == 2 * Self.size)
    }

    @Test func aMissingElementIsAMismatch() {
        #expect(ImageComparison.mismatch(between: image(), and: image(lineAt: 40, thickness: 2)) != nil)
    }

    @Test func aDifferentSizeIsAMismatchWithoutDiffImage() throws {
        let mismatch = try #require(ImageComparison.mismatch(between: image(size: 50), and: image()))
        #expect(mismatch.diff == nil)
        #expect(mismatch.summary.contains("50x50") && mismatch.summary.contains("100x100"))
    }

    @Test func diffImageMarksDifferingPixelsInRed() throws {
        let mismatch = try #require(ImageComparison.mismatch(between: image(lineAt: 41), and: image(lineAt: 40)))
        let diff = try #require(mismatch.diff)
        #expect(diff.width == Self.size && diff.height == Self.size)
        // Pixel (40, 10) lost its ink and (41, 10) gained some: both are flagged red.
        for x in [40, 41] {
            let pixel = (10 * Self.size + x) * RenderedImage.bytesPerPixel
            #expect(Array(diff.bgra[pixel...(pixel + 2)]) == [0, 0, 255], "pixel \(x)")
        }
        // Untouched paper stays white, so red stands out.
        let paper = (10 * Self.size + 5) * RenderedImage.bytesPerPixel
        #expect(Array(diff.bgra[paper...(paper + 2)]) == [255, 255, 255])
    }

    @Test func pngRoundTripKeepsEveryPixel() throws {
        let original = image(lineAt: 33, thickness: 3, grey: 77)
        let decoded = try #require(RenderedImage(pngData: original.pngData()))
        #expect(decoded == original)
    }

    @Test func nonImageDataIsNotDecoded() {
        #expect(RenderedImage(pngData: Data([1, 2, 3])) == nil)
    }
}

extension RenderedImage {
    /// A copy with one pixel's colour channels set to `grey`.
    fileprivate func with(pixel: (x: Int, y: Int), grey: UInt8) -> RenderedImage {
        var copy = bgra
        let start = (pixel.y * width + pixel.x) * Self.bytesPerPixel
        copy[start...(start + 2)] = [grey, grey, grey]
        return RenderedImage(width: width, height: height, bgra: copy)
    }
}
