import SwiftUI

extension View {
    /// A Gallery's "+" action: a toolbar button on the Mac; on iOS a floating
    /// button in the lower right, within thumb's reach, like other native iOS apps.
    func addButton(_ title: LocalizedStringKey, identifier: String, action: @escaping () -> Void) -> some View {
        #if os(iOS)
            overlay(alignment: .bottomTrailing) {
                Button(action: action) {
                    Image(systemName: "plus")
                        .font(.title2.weight(.semibold))
                        .foregroundStyle(.white)
                        .frame(width: 56, height: 56)
                        .background(Color.accentColor, in: Circle())
                        .shadow(radius: 4, y: 2)
                }
                .padding(20)
                .accessibilityLabel(title)
                .accessibilityIdentifier(identifier)
            }
        #else
            toolbar {
                ToolbarItem {
                    Button(title, systemImage: "plus", action: action)
                        .accessibilityIdentifier(identifier)
                }
            }
        #endif
    }
}
