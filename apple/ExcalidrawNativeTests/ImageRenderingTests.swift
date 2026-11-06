import Foundation
import Testing

@testable import ExcalidrawNative

/// Renders images offscreen and probes pixels. The test picture has red at the
/// top left, green at the top right, blue at the bottom left and yellow at the
/// bottom right; paper is white.
@MainActor @Suite struct ImageRenderingTests {
    private typealias Scene = (editor: Editor, geometry: Geometry)

    private let red: [UInt8] = [255, 0, 0]
    private let green: [UInt8] = [0, 255, 0]
    private let blue: [UInt8] = [0, 0, 255]
    private let yellow: [UInt8] = [255, 255, 0]
    private let paper: [UInt8] = [255, 255, 255]
    private let placeholderGrey: [UInt8] = [0xe7, 0xe7, 0xe7]

    /// A Drawing with the test picture as file "q" and the given Elements.
    private func scene(_ elements: String..., files: [String: Data]? = nil) -> Scene {
        TestScenes.scene(withImages: files ?? ["q": TestImages.quadrants()], elements)
    }

    private func render(_ scene: Scene, size: Int = 200, viewport: Viewport = Viewport()) throws -> RenderedImage {
        try OffscreenRender.render(scene.geometry, viewport: viewport, size: size, files: scene.editor).image
    }

    /// A 100 x 100 image at (50, 50), the quadrants 50 points wide.
    private func box(scale: String = "[1,1]", crop: String = "null", _ extra: String = "") -> Scene {
        scene(TestScenes.image("i", 50, 50, (100, 100), scale: scale, crop: crop, extra))
    }

    @Test func anImageFillsItsBoxWithThePicture() throws {
        let frame = try render(box())

        #expect(frame.rgb(75, 75) == red)
        #expect(frame.rgb(125, 75) == green)
        #expect(frame.rgb(75, 125) == blue)
        #expect(frame.rgb(125, 125) == yellow)
        #expect(frame.rgb(40, 100) == paper, "left of the box")
        #expect(frame.rgb(100, 160) == paper, "below the box")
    }

    @Test func theBoxIsScaledAndPlacedWithTheViewport() throws {
        let frame = try render(box(), size: 400, viewport: Viewport(origin: [25, 25], zoom: 2))

        #expect(frame.rgb(60, 60) == red, "the box starts at (50, 50) Scene, (50, 50) device")
        #expect(frame.rgb(200, 200) == yellow)
        #expect(frame.rgb(260, 260) == paper, "past the box's end at (250, 250)")
        #expect(frame.rgb(40, 40) == paper)
    }

    @Test func aMirroredImageShowsItsPictureMirrored() throws {
        let sideways = try render(box(scale: "[-1,1]"))
        #expect(sideways.rgb(75, 75) == green)
        #expect(sideways.rgb(125, 75) == red)
        #expect(sideways.rgb(75, 125) == yellow)

        let upsideDown = try render(box(scale: "[1,-1]"))
        #expect(upsideDown.rgb(75, 75) == blue)
        #expect(upsideDown.rgb(75, 125) == red)
    }

    @Test func aTurnedImageTurnsAboutItsCentre() throws {
        // A quarter turn clockwise takes the top left to the top right.
        let frame = try render(box(#","angle":\#(Double.pi / 2)"#))

        #expect(frame.rgb(125, 75) == red)
        #expect(frame.rgb(125, 125) == green)
        #expect(frame.rgb(75, 125) == yellow)
        #expect(frame.rgb(75, 75) == blue)
    }

    @Test func mirroringHappensBeforeTurning() throws {
        // Excalidraw applies the scale after the rotation to the context, so
        // the picture is mirrored in the box and then the box turned.
        let frame = try render(box(scale: "[-1,1]", #","angle":\#(Double.pi / 2)"#))

        // Mirrored: green left, red right (top row); turned: top row to the right column.
        #expect(frame.rgb(125, 75) == green)
        #expect(frame.rgb(125, 125) == red)
    }

    @Test func aCropShowsOnlyThatPartOfThePicture() throws {
        // The top right quarter of the 40 x 40 pixel file.
        let corner = try render(
            box(crop: #"{"x":20,"y":0,"width":20,"height":20,"naturalWidth":40,"naturalHeight":40}"#))
        #expect(corner.rgb(60, 60) == green)
        #expect(corner.rgb(140, 140) == green)

        // The middle of the file shows a quarter of every colour.
        let middle = try render(
            box(crop: #"{"x":10,"y":10,"width":20,"height":20,"naturalWidth":40,"naturalHeight":40}"#))
        #expect(middle.rgb(60, 60) == red)
        #expect(middle.rgb(140, 60) == green)
        #expect(middle.rgb(60, 140) == blue)
        #expect(middle.rgb(140, 140) == yellow)
    }

    @Test func opacityBlendsTheImageWithWhatIsBehindIt() throws {
        let frame = try render(box(#","opacity":50"#))

        let pixel = frame.rgb(75, 75)
        #expect(pixel[0] == 255 && abs(Int(pixel[1]) - 128) <= 1 && abs(Int(pixel[2]) - 128) <= 1, "\(pixel)")
    }

    @Test func roundedImagesAreClippedAtTheirCorners() throws {
        let rounded = try render(box(#","roundness":{"type":3}"#))
        // The radius is a quarter of the side: 25 points.
        #expect(rounded.rgb(52, 52) == paper, "cut off at the corner")
        #expect(rounded.rgb(75, 52) == red, "straight edges stay")
        #expect(rounded.rgb(60, 60) == red)

        #expect(try render(box()).rgb(52, 52) == red, "sharp without roundness")
    }

    @Test func imagesAreDrawnInZOrderWithShapesAround() throws {
        let cover =
            ##"{"id":"c","type":"rectangle","x":60,"y":60,"width":40,"height":40,"roughness":0,"backgroundColor":"#a5d8ff","fillStyle":"solid","strokeColor":"transparent"}"##
        let image = TestScenes.image("i", 50, 50, (100, 100))

        let under = try render(scene(image, cover))
        #expect(under.rgb(80, 80) == [0xa5, 0xd8, 0xff], "a later shape covers the image")
        #expect(under.rgb(55, 55) == red)

        let over = try render(scene(cover, image))
        #expect(over.rgb(80, 80) == red, "a later image covers the shape")
    }

    @Test func aMissingFileLeavesAPlaceholderInTheBox() throws {
        let frame = try render(scene(TestScenes.image("i", 50, 50, (100, 100), file: "gone")))

        #expect(frame.rgb(52, 52) == placeholderGrey)
        #expect(frame.rgb(148, 52) == placeholderGrey)
        #expect(frame.rgb(40, 40) == paper)
        let icon = (50..<150).flatMap { x in (50..<150).map { frame.rgb(x, $0) } }.filter { $0 == [0x88, 0x88, 0x88] }
        #expect(icon.count > 500, "the picture icon in the middle: \(icon.count)")
    }

    @Test func placeholdersUseExcalidrawsDarkColoursOnTheDarkCanvas() throws {
        let scene = scene(TestScenes.image("i", 50, 50, (100, 100), file: "gone"))
        let renderer = try OffscreenRender.makeRenderer()
        renderer.files = scene.editor
        renderer.isDark = true
        renderer.setGeometry(scene.geometry)
        let frame = OffscreenRender.draw(with: renderer, size: 200)

        #expect(frame.rgb(52, 52) == [0x2e, 0x2e, 0x2e])
        renderer.isDark = false
        #expect(OffscreenRender.draw(with: renderer, size: 200).rgb(52, 52) == placeholderGrey, "and back")
    }

    @Test func anImageWithoutAFileAndOneThatIsNoPictureAlsoGetThePlaceholder() throws {
        let noFile = try render(
            scene(#"{"id":"i","type":"image","x":50,"y":50,"width":100,"height":100,"fileId":null}"#))
        #expect(noFile.rgb(52, 52) == placeholderGrey)

        let broken = try render(
            scene(
                TestScenes.image("i", 50, 50, (100, 100), file: "junk"),
                files: ["junk": Data("<svg></svg>".utf8)]))
        #expect(broken.rgb(52, 52) == placeholderGrey)
    }

    @Test func aFailedImageHasTheCrossedOutIconInsteadOfThePictureIcon() throws {
        let plain = try render(scene(TestScenes.image("i", 50, 50, (100, 100), file: "gone")))
        let failed = try render(scene(TestScenes.image("i", 50, 50, (100, 100), file: "gone", #","status":"error""#)))

        #expect(plain != failed)
        #expect(failed.rgb(52, 52) == placeholderGrey)
    }

    @Test func placeholdersAreTurnedAndMirroredWithTheirBox() throws {
        // A quarter turn about the centre of a wide box makes it tall.
        let frame = try render(
            scene(TestScenes.image("i", 40, 70, (120, 60), file: "gone", #","angle":\#(Double.pi / 2)"#)))

        #expect(frame.rgb(100, 45) == placeholderGrey, "top of the turned box")
        #expect(frame.rgb(100, 155) == placeholderGrey, "bottom of the turned box")
        #expect(frame.rgb(55, 100) == paper, "where the unturned box was")
    }

    @Test func oneTexturePerFileNoMatterHowManyImagesShowIt() throws {
        let scene = scene(
            TestScenes.image("a", 10, 10, (50, 50)), TestScenes.image("b", 100, 10, (50, 50)),
            TestScenes.image("c", 10, 100, (50, 50), scale: "[-1,1]"))

        let (_, renderer) = try OffscreenRender.render(
            scene.geometry, viewport: Viewport(), files: scene.editor)

        #expect(renderer.textures.count == 1)
        #expect(renderer.textures.decodeCount == 1)
    }

    @Test func texturesAreReleasedWhenNoImageShowsTheirFileAnyMore() throws {
        let both = scene(
            TestScenes.image("a", 10, 10, (50, 50)), TestScenes.image("b", 100, 10, (50, 50), file: "r"),
            files: ["q": TestImages.quadrants(), "r": TestImages.quadrants()])
        let (_, renderer) = try OffscreenRender.render(both.geometry, viewport: Viewport(), files: both.editor)
        #expect(renderer.textures.count == 2)

        let one = scene(TestScenes.image("a", 10, 10, (50, 50)))
        renderer.files = one.editor
        renderer.setGeometry(one.geometry)
        _ = OffscreenRender.draw(with: renderer)
        #expect(renderer.textures.count == 1, "r is gone")

        let none = scene(#"{"id":"r","type":"rectangle","width":5,"height":5}"#)
        renderer.setGeometry(none.geometry)
        _ = OffscreenRender.draw(with: renderer)
        #expect(renderer.textures.count == 0)
    }

    @Test func movingAnImageDoesNotDecodeItsFileAgain() throws {
        let first = scene(TestScenes.image("a", 10, 10, (50, 50), #","version":1"#))
        let (_, renderer) = try OffscreenRender.render(first.geometry, viewport: Viewport(), files: first.editor)

        let moved = scene(TestScenes.image("a", 90, 10, (50, 50), #","version":2"#))
        renderer.setGeometry(moved.geometry)
        let frame = OffscreenRender.draw(with: renderer)

        #expect(frame.rgb(100, 20) == red)
        #expect(renderer.textures.decodeCount == 1, "the file's texture is kept for the new version")
    }

    @Test func anotherDrawingsFileOfTheSameIdIsShownWithItsOwnPixels() throws {
        // File ids are only unique within a Drawing.
        let quadrants = scene(TestScenes.image("a", 50, 50, (100, 100)))
        let (_, renderer) = try OffscreenRender.render(
            quadrants.geometry, viewport: Viewport(), size: 200, files: quadrants.editor)

        let allBlue = TestImages.png(width: 4, height: 4) { _, _ in TestImages.blue }
        let other = scene(TestScenes.image("a", 50, 50, (100, 100)), files: ["q": allBlue])
        renderer.files = other.editor
        renderer.setGeometry(other.geometry)
        let frame = OffscreenRender.draw(with: renderer, size: 200)

        #expect(frame.rgb(75, 75) == blue)
    }

    @Test func aFileThatWasMissingIsShownOnceTheFilesComeFromWhereItIs() throws {
        let image = TestScenes.image("a", 50, 50, (100, 100))
        let without = scene(image, files: [:])
        let (placeholder, renderer) = try OffscreenRender.render(
            without.geometry, viewport: Viewport(), size: 200, files: without.editor)
        #expect(placeholder.rgb(60, 60) == placeholderGrey)

        // Same geometry, now with a source that has the file.
        renderer.files = scene(image).editor
        let frame = OffscreenRender.draw(with: renderer, size: 200)

        #expect(frame.rgb(75, 75) == red)
    }

    // MARK: Reference images

    /// Every look an image can have, and the placeholders.
    @Test func images() throws {
        let scene = TestScenes.images()
        let frame = try render(scene, size: 440)

        expectMatchesReference(frame, named: "images")
    }

    @Test func imagesStaySmoothWhenZoomedOutAndSharpWhenZoomedIn() throws {
        let scene = TestScenes.images()

        expectMatchesReference(
            try render(scene, size: 220, viewport: Viewport(origin: [0, 0], zoom: 0.5)), named: "images-zoomed-out")
        expectMatchesReference(
            try render(scene, size: 440, viewport: Viewport(origin: [20, 20], zoom: 3.5)), named: "images-zoomed-in")
    }
}
