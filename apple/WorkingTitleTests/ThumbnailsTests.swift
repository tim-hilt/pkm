import CoreGraphics
import Foundation
import Testing

@testable import WorkingTitle

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
        let before = try entry("sample")

        _ = await Thumbnails(store: store).thumbnail(for: before)

        let png = try #require(store.thumbnail(for: before))
        #expect(RenderedImage(pngData: png)?.width == ThumbnailRenderer.width)
        #expect(try FileManager.default.contentsOfDirectory(atPath: directory.path) == ["sample.excalidraw"])
        #expect(try store.open("sample") == sampleBytes())
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

    /// A busy GPU may fail a draw now and then: that is tried again, a few times.
    @Test func aFailedDrawIsTriedAgain() async throws {
        try store.save(sampleBytes(), as: "sample")
        let real = try #require(ThumbnailRenderer())
        var attempts = 0
        let thumbnails = Thumbnails(store: store, retryDelay: .zero) { scene in
            attempts += 1
            return attempts < 3 ? nil : real.render(scene)
        }

        let image = try await thumbnails.thumbnail(for: entry("sample")).image

        #expect(image != nil)
        #expect(attempts == 3)
        #expect(thumbnails.rendered == 1)
    }

    @Test func aDrawThatKeepsFailingGivesUpAfterAFewTries() async throws {
        try store.save(sampleBytes(), as: "sample")
        var attempts = 0
        let thumbnails = Thumbnails(store: store, retryDelay: .zero) { _ in
            attempts += 1
            return nil
        }

        let thumbnail = try await thumbnails.thumbnail(for: entry("sample"))

        #expect(thumbnail.image == nil && !thumbnail.isUnreadable)
        #expect(attempts == Thumbnails.drawAttempts)
        #expect(thumbnails.rendered == 0)
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
}
