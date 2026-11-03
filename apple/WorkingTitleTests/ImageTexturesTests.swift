import Foundation
import Metal
import Testing

@testable import WorkingTitle

/// The texture cache: one texture per file id, made on demand and released
/// when nothing refers to the file any more.
@MainActor @Suite struct ImageTexturesTests {
    private func makeTextures() throws -> ImageTextures {
        let renderer = try OffscreenRender.makeRenderer()
        return renderer.textures
    }

    @Test func aTextureHasThePixelsOfItsFileAndAFullMipmapChain() throws {
        let textures = try makeTextures()
        let files = FakeFiles(["a": TestImages.quadrants(size: 40)])

        let texture = try #require(textures.texture(forFile: "a", from: files))

        #expect((texture.width, texture.height) == (40, 40))
        #expect(texture.mipmapLevelCount == 6, "40, 20, 10, 5, 2, 1")
        var pixel = [UInt8](repeating: 0, count: 4)
        texture.getBytes(&pixel, bytesPerRow: 4, from: MTLRegionMake2D(30, 5, 1, 1), mipmapLevel: 0)
        #expect(pixel == TestImages.green)
    }

    @Test func aFileIsReadAndDecodedOnceHoweverOftenItIsAskedFor() throws {
        let textures = try makeTextures()
        let files = FakeFiles(["a": TestImages.quadrants()])

        let first = textures.texture(forFile: "a", from: files)
        let second = textures.texture(forFile: "a", from: files)

        #expect(first != nil && first === second)
        #expect(files.requests == ["a"])
        #expect(textures.decodeCount == 1)
    }

    @Test func aMissingFileHasNoTextureAndIsNotAskedForAgain() throws {
        let textures = try makeTextures()
        let files = FakeFiles()

        #expect(textures.texture(forFile: "gone", from: files) == nil)
        #expect(textures.texture(forFile: "gone", from: files) == nil)
        #expect(textures.texture(forFile: "none", from: nil) == nil)

        #expect(files.requests == ["gone"])
        #expect(!textures.isUndecodable("gone"), "missing is not the same as broken")
        #expect(textures.count == 0)
    }

    @Test func aFileThatIsNoImageIsRememberedAsUndecodable() throws {
        let textures = try makeTextures()
        let files = FakeFiles(["junk": Data("<svg/>".utf8)])

        #expect(textures.texture(forFile: "junk", from: files) == nil)
        #expect(textures.texture(forFile: "junk", from: files) == nil)

        #expect(textures.isUndecodable("junk"))
        #expect(textures.decodeCount == 1)
    }

    @Test func textureOfFilesNothingReferencesAnyMoreAreReleased() throws {
        let textures = try makeTextures()
        let files = FakeFiles(["a": TestImages.quadrants(), "b": TestImages.quadrants()])
        _ = textures.texture(forFile: "a", from: files)
        _ = textures.texture(forFile: "b", from: files)
        #expect(textures.count == 2)

        textures.keepOnly(["a"])

        #expect(textures.count == 1)
        #expect(textures.texture(forFile: "a", from: files) != nil)
        #expect(files.requests == ["a", "b"], "the kept one was not read again")
    }

    @Test func forgettingFailuresLetsAFileBeTriedAgain() throws {
        let textures = try makeTextures()
        let files = FakeFiles()
        #expect(textures.texture(forFile: "late", from: files) == nil)

        files.files["late"] = TestImages.quadrants()
        textures.forgetFailures()

        #expect(textures.texture(forFile: "late", from: files) != nil)
    }
}
