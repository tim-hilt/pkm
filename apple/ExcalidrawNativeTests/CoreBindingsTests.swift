import Foundation
import Testing

@testable import ExcalidrawNative

@Suite struct CoreBindingsTests {
    @Test func newEditorHasAnEmptyScene() {
        #expect(Editor().elementCount() == 0)
    }

    @Test func loadedBytesComeBackFromSave() throws {
        let editor = Editor()
        let drawing = Data(
            """
            {"type":"excalidraw","version":2,"elements":[
              {"id":"a","type":"rectangle","x":1,"y":2,"width":3,"height":4,"someFutureProperty":[1,2]}]}
            """.utf8)

        try editor.load(bytes: drawing)

        #expect(editor.elementCount() == 1)
        let saved = String(decoding: editor.save(), as: UTF8.self)
        #expect(saved.contains(#""someFutureProperty": ["#))
        #expect(saved.contains(##""strokeColor": "#1e1e1e""##), "Excalidraw's defaults are added")
    }

    @Test func bytesThatAreNoDrawingThrowAndKeepTheScene() throws {
        let editor = Editor()

        #expect(throws: LoadError.self) { try editor.load(bytes: Data("not json".utf8)) }
        #expect(editor.elementCount() == 0)
    }
}
