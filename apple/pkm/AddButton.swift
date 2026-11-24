import SwiftUI

/// The floating "+" button's geometry on iOS, for views that sit beside it.
enum AddButtonMetrics {
    static let size: CGFloat = 56
    static let margin: CGFloat = 12
}

extension View {
    /// A Gallery's "+" action, also on the "n" key: a toolbar button on the Mac; on iOS a floating
    /// Liquid Glass button in the lower right, within thumb's reach. On iOS it is a bottom inset,
    /// so it sits above the tab bar (full or minimised) and the scrolling content clears it.
    /// While `isCovered` by a sheet it is gone, so that it does not shine through.
    func addButton(
        _ title: LocalizedStringKey, identifier: String, isCovered: Bool = false, action: @escaping () -> Void
    ) -> some View {
        #if os(iOS)
            safeAreaInset(edge: .bottom, alignment: .trailing, spacing: 0) {
                if !isCovered {
                    Button(action: action) {
                        Image(systemName: "plus")
                            .font(.title2.weight(.semibold))
                            .frame(width: AddButtonMetrics.size, height: AddButtonMetrics.size)
                    }
                    .buttonStyle(.glassProminent)
                    .buttonBorderShape(.circle)
                    .tint(.accentColor)
                    .keyboardShortcut("n", modifiers: [])
                    .padding(AddButtonMetrics.margin)
                    .accessibilityLabel(title)
                    .accessibilityIdentifier(identifier)
                }
            }
        #else
            toolbar {
                ToolbarItem {
                    Button(title, systemImage: "plus", action: action)
                        .keyboardShortcut("n", modifiers: [])
                        .accessibilityIdentifier(identifier)
                }
            }
        #endif
    }
}
