import Testing

@testable import ExcalidrawNative

/// The Canvas shows the editor's Scene in the colours of the system
/// appearance and switches live (ADR-0010).
@MainActor @Suite struct CanvasAppearanceTests {
    @Test func aDarkCanvasShowsTheEditorItIsGivenInDarkColours() throws {
        let canvas = CanvasMetalView()
        let renderer = try #require(canvas.renderer)
        canvas.isDark = true
        let editor = Editor()
        canvas.editor = editor

        #expect(editor.darkMode())
        #expect(renderer.isDark)
        #expect(renderer.paperColor == 0x1212_12ff, "white as excalidraw.com's dark theme shows it")
    }

    @Test func switchingTheAppearanceRedrawsWithoutChangingTheScene() throws {
        let canvas = CanvasMetalView()
        let renderer = try #require(canvas.renderer)
        let editor = try SampleDrawing.makeEditor()
        canvas.editor = editor
        let light = try #require(renderer.geometry)
        let revision = editor.revision()

        canvas.isDark = true
        #expect(renderer.geometry !== light)
        #expect(renderer.paperColor == 0x1212_12ff)
        #expect(editor.revision() == revision, "nothing to save")

        canvas.isDark = false
        #expect(!editor.darkMode())
        #expect(renderer.paperColor == 0xffff_ffff)
    }
}
