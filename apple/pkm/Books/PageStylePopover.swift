import SwiftUI

/// The Reader's cog popover: Page Style and Page Colors, shared by all Books and saved,
/// then what is known about `book`.
struct PageStylePopover: View {
    @Environment(ReaderSettings.self) private var settings
    @Environment(\.colorScheme) private var colorScheme
    let book: Book

    var body: some View {
        @Bindable var settings = settings
        // In a ScrollView so the content stays pinned to the top while the popover resizes.
        // Otherwise it is centered in the in-between size and the swatches fly in.
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                Text("Page Style").font(.headline)
                PageStylePicker(selection: $settings.pageStyle, paper: settings.pageColors.paper)
                if settings.arePagesDark(systemScheme: colorScheme) {
                    Text("Dark Page Colors").font(.headline)
                    PageColorsEditor()
                }
                Divider()
                BookInfoList(info: BookInfo(book))
            }
            .padding()
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .scrollBounceBehavior(.basedOnSize)
        .frame(minWidth: 280)
        .presentationCompactAdaptation(.popover)
    }
}
