import Foundation
import ImageIO
import Testing
import UniformTypeIdentifiers

@testable import pkm

#if canImport(UIKit)
    import UIKit
#else
    import AppKit
#endif

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

    @Test func anImageIsCopiedAsTheTypeItIsStored() {
        let jpeg = encoded(.jpeg)
        #if canImport(UIKit)
            let pasteboard = UIPasteboard.withUniqueName()
            defer { UIPasteboard.remove(withName: pasteboard.name) }
            StoredImage.copy(jpeg, to: pasteboard)
            #expect(pasteboard.data(forPasteboardType: UTType.jpeg.identifier) == jpeg)
        #else
            let pasteboard = NSPasteboard.withUniqueName()
            defer { pasteboard.releaseGlobally() }
            StoredImage.copy(jpeg, to: pasteboard)
            #expect(pasteboard.data(forType: NSPasteboard.PasteboardType(UTType.jpeg.identifier)) == jpeg)
            // What reads only the picture types every Mac app takes still finds it.
            #expect(PictureInput.hasPictures(in: pasteboard))
        #endif
    }

    @Test func whatIsNoImageIsRefused() {
        #expect(StoredImage.bytes(of: Data("not an image".utf8)) == nil)
    }
}
