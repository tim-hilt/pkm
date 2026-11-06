import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers

@testable import ExcalidrawNative

/// Image files for tests, made in memory so that no binary fixtures are needed.
enum TestImages {
    static let red: [UInt8] = [255, 0, 0, 255]
    static let green: [UInt8] = [0, 255, 0, 255]
    static let blue: [UInt8] = [0, 0, 255, 255]
    static let yellow: [UInt8] = [255, 255, 0, 255]

    /// A PNG of `width` x `height` pixels, each given by `color(x, y)` as
    /// non-premultiplied RGBA.
    static func png(width: Int, height: Int, color: (Int, Int) -> [UInt8]) -> Data {
        var pixels = [UInt8]()
        for y in 0..<height { for x in 0..<width { pixels += color(x, y) } }
        let image = pixels.withUnsafeMutableBytes { buffer in
            CGContext(
                data: buffer.baseAddress, width: width, height: height, bitsPerComponent: 8,
                bytesPerRow: width * 4, space: CGColorSpace(name: CGColorSpace.sRGB)!,
                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!.makeImage()!
        }
        let data = NSMutableData()
        let destination = CGImageDestinationCreateWithData(data, UTType.png.identifier as CFString, 1, nil)!
        CGImageDestinationAddImage(destination, image, nil)
        precondition(CGImageDestinationFinalize(destination), "PNG encoding failed")
        return data as Data
    }

    /// Red top left, green top right, blue bottom left, yellow bottom right.
    static func quadrants(size: Int = 40) -> Data {
        png(width: size, height: size) { x, y in
            switch (x < size / 2, y < size / 2) {
            case (true, true): red
            case (false, true): green
            case (true, false): blue
            case (false, false): yellow
            }
        }
    }

    /// The `files` member of a Drawing holding `files`, keyed by id, as
    /// excalidraw.com stores them: PNG data URLs.
    static func filesMember(_ files: [String: Data]) -> String {
        let entries = files.sorted { $0.key < $1.key }.map { id, data in
            #""\#(id)":{"mimeType":"image/png","id":"\#(id)","dataURL":"data:image/png;base64,\#(data.base64EncodedString())","created":1}"#
        }
        return "{\(entries.joined(separator: ","))}"
    }
}

/// An `ImageFileSource` over a dictionary, counting how often it is asked.
final class FakeFiles: ImageFileSource {
    var files: [String: Data]
    private(set) var requests: [String] = []

    init(_ files: [String: Data] = [:]) { self.files = files }

    func fileBytes(id: String) -> Data? {
        requests.append(id)
        return files[id]
    }
}

extension RenderedImage {
    /// Red, green and blue at device pixel `(x, y)`.
    func rgb(_ x: Int, _ y: Int) -> [UInt8] {
        let offset = (y * width + x) * Self.bytesPerPixel
        return [bgra[offset + 2], bgra[offset + 1], bgra[offset]]
    }
}
