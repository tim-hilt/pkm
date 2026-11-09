import Foundation
import Testing

@testable import pkm

@Suite struct ImageDecoderTests {
    @Test func aPNGDecodesToTopDownPremultipliedRGBA() throws {
        let png = TestImages.png(width: 2, height: 2) { x, y in
            switch (x, y) {
            case (0, 0): TestImages.red
            case (1, 0): TestImages.green
            case (0, 1): [0, 0, 255, 128]  // half transparent blue
            default: [0, 0, 0, 0]
            }
        }

        let image = try #require(ImageDecoder.decode(png))

        #expect((image.width, image.height) == (2, 2))
        #expect(Array(image.pixels[0..<4]) == [255, 0, 0, 255], "top left")
        #expect(Array(image.pixels[4..<8]) == [0, 255, 0, 255], "top right")
        // Premultiplied: 255 * 128/255, give or take the rounding of the decoder.
        let blue = Array(image.pixels[8..<12])
        #expect(blue[0] == 0 && blue[1] == 0 && blue[3] == 128 && abs(Int(blue[2]) - 128) <= 1, "\(blue)")
        #expect(Array(image.pixels[12..<16]) == [0, 0, 0, 0], "bottom right")
    }

    @Test func largeImagesAreDownscaledKeepingTheirProportions() throws {
        let png = TestImages.png(width: 300, height: 100) { _, _ in TestImages.red }

        let image = try #require(ImageDecoder.decode(png, maxPixelSize: 150))

        #expect((image.width, image.height) == (150, 50))
        #expect(Array(image.pixels[0..<4]) == [255, 0, 0, 255])
    }

    @Test func smallImagesAreNotEnlarged() throws {
        let png = TestImages.png(width: 30, height: 10) { _, _ in TestImages.red }

        let image = try #require(ImageDecoder.decode(png))

        #expect((image.width, image.height) == (30, 10))
    }

    @Test func bytesThatAreNoImageDoNotDecode() {
        #expect(ImageDecoder.decode(Data("not an image".utf8)) == nil)
        #expect(ImageDecoder.decode(Data()) == nil)
        #expect(ImageDecoder.decode(Data(TestImages.quadrants().prefix(20))) == nil, "a truncated file")
    }
}
