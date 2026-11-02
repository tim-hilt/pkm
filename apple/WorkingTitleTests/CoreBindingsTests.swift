import Testing

@testable import WorkingTitle

@Suite struct CoreBindingsTests {
    @Test func newEditorHasAnEmptyScene() {
        #expect(Editor().elementCount() == 0)
    }
}
