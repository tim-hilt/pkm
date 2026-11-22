import Foundation
import Testing

@testable import pkm

/// SVGs drawn as browsers draw them, which ImageIO can't.
@MainActor @Suite struct SVGRendererTests {
    /// Sized like many on the web: by its viewBox alone. Red on top, the bottom empty.
    static let redTopHalf = Data(
        """
        <?xml version="1.0"?>
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 30 20" width="100%" height="auto">
        <rect width="30" height="10" fill="red"/></svg>
        """.utf8)

    let renderer = SVGRenderer()

    @Test func anSVGIsDrawnUprightAtTwiceItsSize() async throws {
        let png = try #require(await renderer.png(of: Self.redTopHalf))
        let image = try #require(ImageDecoder.decode(png))
        #expect((image.width, image.height) == (60, 40))
        #expect(Array(image.pixels[0..<4]) == TestImages.red)
        #expect(image.pixels[(image.pixels.count - 4)...].allSatisfy { $0 == 0 })
    }

    @Test func htmlInAnSVGIsDrawn() async throws {
        // Like diagrams.net's labels: HTML in a foreignObject.
        let svg = Data(
            """
            <svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><foreignObject width="10" height="10">
            <div xmlns="http://www.w3.org/1999/xhtml" style="width:10px;height:10px;background:red"></div>
            </foreignObject></svg>
            """.utf8)
        let image = try #require(await renderer.png(of: svg).flatMap { ImageDecoder.decode($0) })
        #expect(Array(image.pixels[0..<4]) == TestImages.red)
    }

    @Test func aGiantSVGIsDrawnAtMost4096PixelsLongAndTheNextAsWell() async throws {
        let giant = Data(#"<svg xmlns="http://www.w3.org/2000/svg" width="8000" height="100"/>"#.utf8)
        let image = try #require(await renderer.png(of: giant).flatMap { ImageDecoder.decode($0) })
        #expect((image.width, image.height) == (4096, 51))
        #expect(await renderer.png(of: Self.redTopHalf) != nil)
    }

    @Test func whatIsNoSVGIsRefused() async {
        #expect(await renderer.png(of: Data("<html><p>Not found</p></html>".utf8)) == nil)
        #expect(await renderer.png(of: Data(#"<svg xmlns="http://www.w3.org/2000/svg"><broken"#.utf8)) == nil)
    }
}
