import Foundation
import ImageIO
import Testing
import UniformTypeIdentifiers

@testable import pkm

/// Which bytes an Image is stored as; reading the formats is ImageIO's.
@Suite struct StoredImageTests {
    private func encoded(_ type: UTType) -> Data {
        let source = CGImageSourceCreateWithData(
            TestImages.png(width: 2, height: 2, color: { _, _ in TestImages.red }) as CFData, nil)!
        let output = NSMutableData()
        let destination = CGImageDestinationCreateWithData(output, type.identifier as CFString, 1, nil)!
        CGImageDestinationAddImage(destination, CGImageSourceCreateImageAtIndex(source, 0, nil)!, nil)
        precondition(CGImageDestinationFinalize(destination))
        return output as Data
    }

    private func type(of data: Data) -> UTType? {
        CGImageSourceCreateWithData(data as CFData, nil)
            .flatMap(CGImageSourceGetType).flatMap { UTType($0 as String) }
    }

    @Test func aPNGIsKeptByteForByte() {
        let png = TestImages.png(width: 3, height: 2, color: { _, _ in TestImages.green })
        #expect(StoredImage.bytes(of: png) == png)
    }

    @Test func aJPEGAndAGIFAreKeptByteForByte() {
        for kind in [UTType.jpeg, .gif] {
            let data = encoded(kind)
            #expect(StoredImage.bytes(of: data) == data)
        }
    }

    @Test func aTIFFIsStoredAsPNG() throws {
        let tiff = encoded(.tiff)
        #expect(type(of: tiff) == .tiff)
        let stored = try #require(StoredImage.bytes(of: tiff))
        #expect(type(of: stored) == .png)
    }

    @Test func whatIsNoImageIsRefused() {
        #expect(StoredImage.bytes(of: Data("not an image".utf8)) == nil)
    }
}
