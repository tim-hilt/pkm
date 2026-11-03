import Foundation
import ImageIO
import Testing
import UniformTypeIdentifiers

@testable import WorkingTitle

@Suite struct ImageImporterTests {
    /// Pixels no encoder can shrink: deterministic noise.
    private func noise(width: Int, height: Int) -> Data {
        var state: UInt32 = 12345
        return TestImages.png(width: width, height: height) { _, _ in
            state = state &* 1_664_525 &+ 1_013_904_223
            return [
                UInt8(truncatingIfNeeded: state >> 24), UInt8(truncatingIfNeeded: state >> 16),
                UInt8(truncatingIfNeeded: state >> 8), 255,
            ]
        }
    }

    /// `data` re-encoded as `type`.
    private func encoded(_ data: Data, as type: UTType, quality: Double = 1) -> Data {
        let source = CGImageSourceCreateWithData(data as CFData, nil)!
        let image = CGImageSourceCreateImageAtIndex(source, 0, nil)!
        let output = NSMutableData()
        let destination = CGImageDestinationCreateWithData(output, type.identifier as CFString, 1, nil)!
        CGImageDestinationAddImage(
            destination, image, [kCGImageDestinationLossyCompressionQuality: quality] as CFDictionary)
        precondition(CGImageDestinationFinalize(destination))
        return output as Data
    }

    private func pixelSize(of data: Data) throws -> (UInt32, UInt32) {
        let image = try #require(ImageDecoder.decode(data, maxPixelSize: 100_000))
        return (UInt32(image.width), UInt32(image.height))
    }

    @Test func anImageWithinTheLimitsIsKeptExactlyAsItIs() throws {
        let png = TestImages.quadrants(size: 40)

        let file = try #require(ImageImporter.prepare(png))

        #expect(file.bytes == png)
        #expect(file.mimeType == "image/png")
        #expect((file.width, file.height) == (40, 40))
    }

    @Test func theFileIdIsTheSHA1OfTheBytesAsTheyCame() {
        // The test vector of FIPS 180: SHA-1("abc").
        #expect(ImageImporter.fileID(for: Data("abc".utf8)) == "a9993e364706816aba3e25717850c26c9cd0d89d")
    }

    @Test func aWideImageIsDownscaledToTheLimitOnItsLongSideKeepingItsProportions() throws {
        let png = TestImages.png(width: 3000, height: 1000) { x, _ in x < 1500 ? TestImages.red : TestImages.blue }

        let file = try #require(ImageImporter.prepare(png))

        #expect((file.width, file.height) == (1440, 480))
        #expect(try pixelSize(of: file.bytes) == (1440, 480), "the bytes are what the size says")
        #expect(file.mimeType == "image/png")
        #expect(file.fileId == ImageImporter.fileID(for: png), "the id is of the original, as on excalidraw.com")
    }

    @Test func aTallImageIsDownscaledToTheLimitOnItsHeight() throws {
        let png = TestImages.png(width: 500, height: 2880) { _, _ in TestImages.green }

        let file = try #require(ImageImporter.prepare(png))

        #expect((file.width, file.height) == (250, 1440))
    }

    @Test func anImageOverTheSizeLimitIsShrunkUntilItFits() throws {
        // Noise does not compress: 1600 x 1000 pixels are 6 MiB as a PNG.
        let png = noise(width: 1600, height: 1000)
        #expect(png.count > ImageImporter.maxFileSize)

        let file = try #require(ImageImporter.prepare(png))

        #expect(file.bytes.count <= ImageImporter.maxFileSize)
        #expect(max(file.width, file.height) <= ImageImporter.maxPixelSize)
        #expect(try pixelSize(of: file.bytes) == (file.width, file.height))
        #expect(file.width * 10 == file.height * 16, "proportions kept")
    }

    @Test func aJPEGOverTheSizeLimitIsRecompressedAsJPEG() throws {
        let jpeg = encoded(noise(width: 1400, height: 1400), as: .jpeg)
        #expect(jpeg.count > ImageImporter.maxFileSize)

        let file = try #require(ImageImporter.prepare(jpeg))

        #expect(file.mimeType == "image/jpeg")
        #expect(file.bytes.count <= ImageImporter.maxFileSize)
        #expect(try pixelSize(of: file.bytes) == (file.width, file.height))
    }

    @Test func aFormatTheWebCannotReadIsConvertedEvenWhenSmall() throws {
        // The Mac's pasteboard offers screenshots as TIFF.
        let tiff = encoded(TestImages.quadrants(size: 40), as: .tiff)

        let file = try #require(ImageImporter.prepare(tiff))

        #expect(["image/png", "image/jpeg"].contains(file.mimeType))
        #expect((file.width, file.height) == (40, 40))
        #expect(try pixelSize(of: file.bytes) == (40, 40))
    }

    @Test func bytesThatAreNoImageAreNotImported() {
        #expect(ImageImporter.prepare(Data("not an image".utf8)) == nil)
        #expect(ImageImporter.prepare(Data()) == nil)
    }
}
