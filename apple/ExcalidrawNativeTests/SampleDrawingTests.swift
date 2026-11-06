import Foundation
import Testing

@testable import ExcalidrawNative

/// The Drawing bundled with the app opens, shows its Elements and
/// survives load -> save unchanged.
@Suite struct SampleDrawingTests {
    @Test func showsEveryNonDeletedElementOfASupportedType() throws {
        let editor = try SampleDrawing.makeEditor()

        // 8 of the file's 10 Elements: one is deleted, one has an unknown type.
        #expect(editor.elementCount() == 8)
        let geometry = editor.geometry()
        // The arrow is three parts: its line and the two strokes of its head.
        // The text is a line of text, not a part.
        #expect(geometry.partCount() == 9)
        #expect(geometry.textLineCount() == 1)
    }

    @Test func savesBackExactlyWhatWasBundled() throws {
        let bytes = try Data(contentsOf: SampleDrawing.url)
        let editor = try SampleDrawing.makeEditor()

        #expect(editor.save() == bytes)
    }
}
