import Foundation
import Testing

@testable import WorkingTitle

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
    nonisolated private static let names = ["shapes", "strokes", "lines", "freedraw", "text", "images"]

    @Test(arguments: names) func theFixtureExportsAsPNG(name: String) throws {
        let editor = Editor.withExcalifont()
        try editor.load(bytes: Data(contentsOf: Self.fixtures.appending(path: "\(name).excalidraw")))
        let export = try #require(editor.export(scope: .scene))
        let image = try #require(PNGExport.render(editor, scope: .scene, background: true))

        #expect(image.width == Int((export.width * 2).rounded(.up)))
        #expect(image.pixel(2, 2) == (255, 255, 255), "the padding is paper")

        if ProcessInfo.processInfo.environment["RECORD_VISUAL_PARITY"] != nil {
            try image.pngData().write(to: Self.output.appending(path: "\(name)-app.png"))
        }
    }
}
