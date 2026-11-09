import CoreGraphics
import Foundation
import Testing

@testable import pkm

/// Seams 2 and 3 together: the Gallery's thumbnails come from the cache outside
/// the Drawings' folder, are drawn by the renderer only when the cache has no
/// current one, and never touch the `.excalidraw` files.
@MainActor @Suite struct ThumbnailsTests {
    private let directory: URL
    private let store: DrawingStore

    init() throws {
        let root = FileManager.default.temporaryDirectory
            .appending(path: "ThumbnailsTests-\(UUID().uuidString)", directoryHint: .isDirectory)
        directory = root.appending(path: "Documents", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        store = DrawingStore(directory: directory, cacheDirectory: root.appending(path: "Caches"))
    }

    private func entry(_ name: String) throws -> DrawingStore.Entry {
        try #require(store.list().first { $0.name == name })
    }

    private func sampleBytes() throws -> Data { try Data(contentsOf: SampleDrawing.url) }

    /// Whether the picture has anything but paper in it.
    private func hasInk(_ image: CGImage) throws -> Bool {
        let rendered = try #require(RenderedImage(cgImage: image))
        return rendered.bgra.indices.contains { $0 % RenderedImage.bytesPerPixel != 3 && rendered.bgra[$0] < 200 }
    }

    @Test func aDrawingGetsAThumbnailOfTheCardSize() async throws {
        try store.save(sampleBytes(), as: "sample")
        let thumbnails = Thumbnails(store: store)

        let image = try await #require(thumbnails.thumbnail(for: entry("sample")).image)

        #expect(image.width == ThumbnailRenderer.width)
        #expect(image.height == ThumbnailRenderer.height)
        #expect(try hasInk(image))
        #expect(thumbnails.rendered == 1)
    }

    @Test func theThumbnailIsCachedOutsideTheDrawingsFolder() async throws {
        try store.save(sampleBytes(), as: "sample")
        let before = try entry("sample")  // lists, which gives the Drawing an Item ID
        let stored = try store.open("sample")

        _ = await Thumbnails(store: store).thumbnail(for: before)

        let png = try #require(store.thumbnail(for: before))
        #expect(RenderedImage(pngData: png)?.width == ThumbnailRenderer.width)
        #expect(try FileManager.default.contentsOfDirectory(atPath: directory.path) == ["sample.excalidraw"])
        #expect(try store.open("sample") == stored)
        #expect(try entry("sample") == before)
    }

    @Test func aCachedThumbnailIsShownWithoutDrawingItAgain() async throws {
        try store.save(sampleBytes(), as: "sample")
        _ = try await Thumbnails(store: store).thumbnail(for: entry("sample"))

        let later = Thumbnails(store: store)  // as after a restart of the app
        let image = try await later.thumbnail(for: entry("sample")).image

        #expect(image != nil)
        #expect(later.rendered == 0)
    }

    /// Not the same object: the system may drop an image from memory at any
    /// time (the tests run beside each other), and the disk cache then answers.
    @Test func askingAgainDoesNotDrawAgain() async throws {
        try store.save(sampleBytes(), as: "sample")
        let thumbnails = Thumbnails(store: store)

        let first = try await thumbnails.thumbnail(for: entry("sample")).image
        let second = try await thumbnails.thumbnail(for: entry("sample")).image

        #expect(first != nil && second != nil)
        #expect(first?.width == second?.width && first?.height == second?.height)
        #expect(thumbnails.rendered == 1)
    }

    @Test func aChangedDrawingGetsAFreshThumbnail() async throws {
        try store.save(Editor().save(), as: "a")
        try FileManager.default.setAttributes(
            [.modificationDate: Date(timeIntervalSince1970: 1000)],
            ofItemAtPath: directory.appending(path: "a.excalidraw").path)
        let thumbnails = Thumbnails(store: store)
        let blank = try await #require(thumbnails.thumbnail(for: entry("a")).image)
        #expect(try !hasInk(blank))

        try store.save(sampleBytes(), as: "a")
        let changed = try await #require(thumbnails.thumbnail(for: entry("a")).image)

        #expect(try hasInk(changed))
        #expect(thumbnails.rendered == 2)
    }

    @Test func aFileThatIsNoDrawingIsUnreadableAndStaysUntouched() async throws {
        let junk = Data("{\"elements\": [".utf8)
        try store.save(junk, as: "broken")
        let thumbnails = Thumbnails(store: store)
        let broken = try entry("broken")

        #expect(await thumbnails.thumbnail(for: broken).isUnreadable)
        #expect(await thumbnails.thumbnail(for: broken).isUnreadable, "also when asked again")
        #expect(try store.open("broken") == junk)
        #expect(try entry("broken") == broken)
        #expect(thumbnails.rendered == 0)
        #expect(store.thumbnail(for: broken) == nil)
    }

    @Test func aFileThatWasFixedIsNoLongerUnreadable() async throws {
        try store.save(Data("not json".utf8), as: "a")
        let thumbnails = Thumbnails(store: store)
        let broken = try entry("a")
        #expect(await thumbnails.thumbnail(for: broken).isUnreadable)
        try FileManager.default.setAttributes(
            [.modificationDate: Date(timeIntervalSince1970: 1000)],
            ofItemAtPath: directory.appending(path: "a.excalidraw").path)

        try store.save(sampleBytes(), as: "a")

        let fixed = try entry("a")
        #expect(await thumbnails.thumbnail(for: fixed).image != nil)
    }

    @Test func aDrawingThatIsGoneHasNoThumbnail() async throws {
        try store.save(sampleBytes(), as: "gone")
        let listed = try entry("gone")
        try store.delete("gone")

        #expect(await Thumbnails(store: store).thumbnail(for: listed).image == nil)
    }

    @Test func manyDrawingsWithCachedThumbnailsAreNotDrawnAgain() async throws {
        let png = try #require(ThumbnailRenderer()?.render(drawing: sampleBytes())).pngData()
        for number in 0..<150 {
            try store.save(Data("{}".utf8), as: "drawing \(number)")
            try store.saveThumbnail(png, for: entry("drawing \(number)"))
        }
        let thumbnails = Thumbnails(store: store)

        for listed in try store.list() { _ = await thumbnails.thumbnail(for: listed) }

        #expect(thumbnails.rendered == 0)
    }

    /// The blue channel of the picture's corner pixel: the paper.
    private func paper(_ image: CGImage) throws -> UInt8 {
        let rendered = try #require(RenderedImage(cgImage: image))
        return rendered.bgra[0]
    }

    @Test func aDarkThumbnailShowsTheDarkCanvas() async throws {
        try store.save(sampleBytes(), as: "sample")
        let thumbnails = Thumbnails(store: store)

        let light = try await #require(thumbnails.thumbnail(for: entry("sample")).image)
        let dark = try await #require(thumbnails.thumbnail(for: entry("sample"), dark: true).image)

        #expect(try paper(light) == 255)
        #expect(try paper(dark) < 60)
        #expect(thumbnails.rendered == 2)
    }

    @Test func eachAppearanceIsCachedOnceAndSurvivesARestart() async throws {
        try store.save(sampleBytes(), as: "sample")
        let listed = try entry("sample")
        let first = Thumbnails(store: store)
        _ = await first.thumbnail(for: listed, dark: true)
        #expect(store.thumbnail(for: listed) == nil, "the light one is made when first needed")
        _ = await first.thumbnail(for: listed)

        let later = Thumbnails(store: store)
        let dark = try await #require(later.thumbnail(for: listed, dark: true).image)
        let light = try await #require(later.thumbnail(for: listed).image)

        #expect(try paper(dark) < 60 && paper(light) == 255)
        #expect(later.rendered == 0)
    }

    @Test func aChangedDrawingInvalidatesBothAppearances() async throws {
        try store.save(Editor().save(), as: "a")
        try FileManager.default.setAttributes(
            [.modificationDate: Date(timeIntervalSince1970: 1000)],
            ofItemAtPath: directory.appending(path: "a.excalidraw").path)
        let thumbnails = Thumbnails(store: store)
        let before = try entry("a")
        _ = await thumbnails.thumbnail(for: before)
        _ = await thumbnails.thumbnail(for: before, dark: true)

        try store.save(sampleBytes(), as: "a")
        let changed = try entry("a")

        #expect(store.thumbnail(for: changed) == nil && store.thumbnail(for: changed, dark: true) == nil)
        let light = try await #require(thumbnails.thumbnail(for: changed).image)
        let dark = try await #require(thumbnails.thumbnail(for: changed, dark: true).image)
        #expect(try hasInk(light))
        #expect(try paper(dark) < 60)
        #expect(thumbnails.rendered == 4)
    }
}
