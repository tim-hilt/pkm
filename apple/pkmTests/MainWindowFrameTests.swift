#if os(macOS)
    import Foundation
    import Testing

    @testable import pkm

    /// The Mac window's first-launch frame: big enough for the desktop layout
    /// (spec story 70), and a frame the user chose is left alone.
    @Suite struct MainWindowFrameTests {
        @Test func opensAt1280By800CentredOnALargeScreen() {
            let visible = CGRect(x: 0, y: 0, width: 1680, height: 1000)
            #expect(MainWindowFrame.initial(in: visible) == CGRect(x: 200, y: 100, width: 1280, height: 800))
        }

        @Test func fillsASmallScreen() {
            let visible = CGRect(x: 0, y: 40, width: 1024, height: 700)
            #expect(MainWindowFrame.initial(in: visible) == visible)
        }

        @Test func isCentredOnAScreenWithAnOrigin() {
            let visible = CGRect(x: 1680, y: 0, width: 2000, height: 1200)
            #expect(MainWindowFrame.initial(in: visible) == CGRect(x: 2040, y: 200, width: 1280, height: 800))
        }

        @Test func onlyAFirstLaunchHasNoSavedFrame() throws {
            let defaults = try #require(UserDefaults(suiteName: "MainWindowFrameTests-\(UUID())"))
            #expect(MainWindowFrame.isFirstLaunch(defaults: defaults))
            defaults.set("830 480 700 450 0 0 2560 1410 ", forKey: MainWindowFrame.savedFrameKey)
            #expect(!MainWindowFrame.isFirstLaunch(defaults: defaults))
        }
    }
#endif
