import SwiftUI

/// The floating "+" button's geometry on iOS, for views that sit beside it.
enum AddButtonMetrics {
    static let size: CGFloat = 56
    static let margin: CGFloat = 12
}

extension View {
    /// A Gallery's "+" action, also on the "n" key: a toolbar button on the Mac; on iOS a floating
    /// button in the lower right, within thumb's reach, like other native iOS apps.
    func addButton(_ title: LocalizedStringKey, identifier: String, action: @escaping () -> Void) -> some View {
        #if os(iOS)
            overlay(alignment: .bottomTrailing) {
                Button(action: action) {
                    Image(systemName: "plus")
                        .font(.title2.weight(.semibold))
                        .foregroundStyle(.white)
                        .frame(width: AddButtonMetrics.size, height: AddButtonMetrics.size)
                        .background(Color.accentColor, in: Circle())
                        .shadow(radius: 4, y: 2)
                }
                .keyboardShortcut("n", modifiers: [])
                .padding(AddButtonMetrics.margin)
                .accessibilityLabel(title)
                .accessibilityIdentifier(identifier)
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
