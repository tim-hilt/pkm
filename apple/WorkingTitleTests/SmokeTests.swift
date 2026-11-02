import Testing

@testable import WorkingTitle

@Suite struct SmokeTests {
    @Test func appLinks() {
        #expect(ContentView().body is Never == false)
    }
}
