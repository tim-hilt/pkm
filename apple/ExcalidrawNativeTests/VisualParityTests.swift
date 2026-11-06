import Foundation
import Testing

@testable import ExcalidrawNative

/// The visual parity fixtures (scripts/visual-parity/fixtures) open and export
/// like on excalidraw.com: a PNG at 2x with 10 units of padding. With
/// `RECORD_VISUAL_PARITY` set (`make visual-parity-app`), each export is
/// written to docs/visual-parity/<name>-app.png for the manual comparison
/// described in docs/visual-parity.md.
@MainActor @Suite struct VisualParityTests {
    private static let repository = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
    private static let fixtures = repository.appending(path: "scripts/visual-parity/fixtures")
    private static let output = repository.appending(path: "docs/visual-parity")
    /// Each fixture and the size of the PNG excalidraw.com exports it at
    /// (docs/visual-parity.md).
    nonisolated private static let fixtureSizes = [
        ("shapes", 1400, 1220), ("strokes", 1304, 1209), ("lines", 1163, 1001),
        ("freedraw", 700, 700), ("text", 1162, 780), ("images", 1289, 809),
    ]
    nonisolated private static let names = fixtureSizes.map(\.0)

    @Test(arguments: names) func theFixtureExportsAsPNG(name: String) throws {
        let editor = Editor.withExcalifont()
        try editor.load(bytes: Data(contentsOf: Self.fixtures.appending(path: "\(name).excalidraw")))
        let image = try #require(PNGExport.render(editor, scope: .scene, background: true))

        let size = try #require(Self.fixtureSizes.first { $0.0 == name })
        #expect((image.width, image.height) == (size.1, size.2), "excalidraw.com's size")
        #expect(image.pixel(2, 2) == (255, 255, 255), "the padding is paper")

        if ProcessInfo.processInfo.environment["RECORD_VISUAL_PARITY"] != nil {
            try image.pngData().write(to: Self.output.appending(path: "\(name)-app.png"))
        }
    }
}
