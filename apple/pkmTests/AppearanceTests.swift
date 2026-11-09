import Foundation
import Testing

@testable import pkm

/// The Appearance setting: System, Light or Dark, kept in the settings.
@MainActor @Suite struct AppearanceTests {
    private let settings = UserDefaults.isolated()

    @Test func followsTheSystemUntilTheUserPicksOne() {
        #expect(Appearance.stored(in: settings) == .system)
    }

    @Test func aPickedAppearanceIsKept() {
        settings.set(Appearance.dark.rawValue, forKey: Appearance.key)
        #expect(Appearance.stored(in: settings) == .dark)
    }

    @Test func anUnknownStoredValueFollowsTheSystem() {
        settings.set("sepia", forKey: Appearance.key)
        #expect(Appearance.stored(in: settings) == .system)
    }

    @Test func offersSystemLightAndDarkInThatOrder() {
        #expect(Appearance.allCases == [.system, .light, .dark])
    }

    #if os(macOS)
        @Test func setsTheAppsAppearanceOnAMac() {
            #expect(Appearance.system.nsAppearance == nil)
            #expect(Appearance.light.nsAppearance?.name == .aqua)
            #expect(Appearance.dark.nsAppearance?.name == .darkAqua)
        }
    #else
        @Test func setsTheWindowsStyleOnIPhoneAndIPad() {
            #expect(Appearance.system.userInterfaceStyle == .unspecified)
            #expect(Appearance.light.userInterfaceStyle == .light)
            #expect(Appearance.dark.userInterfaceStyle == .dark)
        }
    #endif
}
