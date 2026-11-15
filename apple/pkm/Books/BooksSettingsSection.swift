import SwiftUI

/// The Books settings page, opened from Settings' Contexts: the Page Style and Page Colors of the Reader, then
/// the Highlight Colors, the selected one being the Default Highlight Color.
/// The Reader shares these, so changing one shows there at once; "Match
/// Appearance" follows the app's Appearance, as the Reader does.
struct BooksSettingsSection: View {
    @Environment(ReaderSettings.self) private var reader
    @Environment(LibraryModel.self) private var library
    @State private var isEditingColors = false

    var body: some View {
        @Bindable var reader = reader
        Section("Page Style") {
            Picker("Page Style", selection: $reader.pageStyle) {
                ForEach(PageStyle.allCases) { Text($0.title).tag($0) }
            }
            .accessibilityIdentifier("page-style")
        }
        Section("Page Colors") {
            PageColorsEditor()
        }
        Section {
            HighlightColorGrid(
                colors: library.highlightColors, selected: library.defaultHighlightColor.tint,
                onSelect: { color in Task { await library.setDefaultHighlightColor(color) } },
                onRemove: { color in Task { await library.removeCustomColor(color) } },
                isEditing: isEditingColors,
                trailing: {
                    AddCustomColorButton { color in Task { await library.addCustomColor(color) } }
                }
            )
            if library.highlightColors.contains(where: \.isCustom) {
                Button(isEditingColors ? "Done" : "Edit Custom Colors") { isEditingColors.toggle() }
            }
        } header: {
            Text("Highlight Colors")
        } footer: {
            Text("New Highlights get the selected color.")
        }
        .onChange(of: library.highlightColors) {
            if !library.highlightColors.contains(where: \.isCustom) { isEditingColors = false }
        }
    }
}
