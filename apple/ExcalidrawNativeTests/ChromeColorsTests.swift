import Testing

@testable import ExcalidrawNative

/// The chrome's colours in the two appearances: Excalidraw's light values
/// stay as they were, Dark mode uses its dark theme's.
@Suite struct ChromeColorsTests {
    @Test func theIslandIsWhiteInLightAndExcalidrawsDarkGreyInDark() {
        #expect(Chrome.rgb(Chrome.island, dark: false) == [0xFF, 0xFF, 0xFF])
        #expect(Chrome.rgb(Chrome.island, dark: true) == [0x23, 0x23, 0x29])
    }

    @Test func checkedControlsKeepTheirLightColoursAndUseTheDarkAccentInDark() {
        #expect(Chrome.rgb(Chrome.checkedFill, dark: false) == [0xE0, 0xDF, 0xFF])
        #expect(Chrome.rgb(Chrome.checkedGlyph, dark: false) == [0x03, 0x00, 0x64])
        #expect(Chrome.rgb(Chrome.checkedFill, dark: true) == [0x40, 0x3E, 0x6A])
        #expect(Chrome.rgb(Chrome.checkedGlyph, dark: true) == [0xE0, 0xDF, 0xFF])
    }
}
