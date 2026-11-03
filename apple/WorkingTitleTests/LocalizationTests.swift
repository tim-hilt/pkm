import Foundation
import Testing

@testable import WorkingTitle

/// User-facing text comes from the String Catalog: a key missing from it
/// falls back to the default value, which these tests catch.
@MainActor @Suite struct LocalizationTests {
    private let missing = "__missing__"

    private func isInCatalog(_ resource: LocalizedStringResource) -> Bool {
        Bundle.main.localizedString(forKey: resource.key, value: missing, table: nil) != missing
    }

    @Test func everyMenuCommandTitleIsInTheStringCatalog() {
        for entry in MenuShortcut.all {
            let title = CanvasCommands.title(entry.command)
            #expect(isInCatalog(title), "\(entry.command): \(title.key)")
        }
    }

    @Test func colourSwatchesHaveSpokenNamesInTheStringCatalog() {
        for color in PropertiesPanel.strokeColors + PropertiesPanel.backgroundColors {
            let name = PropertiesPanel.colorName(color)
            #expect(!name.key.hasPrefix("#"), "\(color) is read out as its hex value")
            #expect(isInCatalog(name), "\(color): \(name.key)")
        }
    }

    @Test func menuTitlesAreEnglish() {
        #expect(String(localized: CanvasCommands.title(.zoomToFit)) == "Zoom to Fit")
        #expect(String(localized: CanvasCommands.title(.setTool(.freedraw))) == "Draw")
    }
}
