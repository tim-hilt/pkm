import SwiftUI

/// The floating "+" button's geometry (iOS with the tab bar on top), for views that sit beside it.
enum AddButtonMetrics {
    static let size: CGFloat = 56
    static let margin: CGFloat = 12
}

extension View {
    /// A screen's "+" action, also on the "n" key, where the thumb or pointer expects it:
    /// - iOS with a bottom tab bar (compact width): the tab bar's bottom accessory, set by
    ///   `addButtonAccessory()`. It sits above the tab bar and, while that is minimised, beside it
    ///   at the same height.
    /// - iOS with the tab bar on top: a floating Liquid Glass button in the lower right, as a
    ///   bottom inset so that the scrolling content clears it.
    /// - Mac: a toolbar button.
    ///
    /// While `isCovered` by a sheet it is gone, so that it does not shine through.
    func addButton(
        _ title: LocalizedStringKey, identifier: String, isCovered: Bool = false, action: @escaping () -> Void
    ) -> some View {
        #if os(iOS)
            modifier(
                AddButtonPlacement(
                    button: isCovered ? nil : AddButton(title: title, identifier: identifier, action: action)))
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

#if os(iOS)
    extension View {
        /// Shows, on a `TabView` with a bottom tab bar, the `addButton` of the screen in front.
        func addButtonAccessory() -> some View { modifier(AddButtonAccessory()) }
    }

    /// A screen's "+": what it says and does.
    private struct AddButton: Equatable {
        let title: LocalizedStringKey
        let identifier: String
        let action: () -> Void

        /// By what it shows only: an action can't be compared, and a screen's "+" keeps its
        /// action, as that reads the screen's state only when it runs.
        static func == (a: Self, b: Self) -> Bool { a.title == b.title && a.identifier == b.identifier }
    }

    /// The `addButton` of the screen in front, handed up to `addButtonAccessory()`.
    private struct AddButtonKey: PreferenceKey {
        static var defaultValue: AddButton? { nil }

        /// Only the screen in front hands one up (`AddButtonPlacement`), so there is one at most.
        static func reduce(value: inout AddButton?, nextValue: () -> AddButton?) {
            value = value ?? nextValue()
        }
    }

    /// Hands `button` up to the bottom tab bar, or floats it where the tab bar is on top.
    private struct AddButtonPlacement: ViewModifier {
        let button: AddButton?
        @Environment(\.horizontalSizeClass) private var sizeClass
        /// Whether the screen is in front: a screen pushed over it and the other tabs stay in
        /// the hierarchy, and with them their preferences, but they disappear.
        @State private var isShown = false

        func body(content: Content) -> some View {
            if sizeClass == .compact {
                content
                    .preference(key: AddButtonKey.self, value: isShown ? button : nil)
                    .onAppear { isShown = true }
                    .onDisappear { isShown = false }
            } else {
                content.safeAreaInset(edge: .bottom, alignment: .trailing, spacing: 0) {
                    if let button {
                        Button(action: button.action) {
                            Image(systemName: "plus")
                                .font(.title2.weight(.semibold))
                                .frame(width: AddButtonMetrics.size, height: AddButtonMetrics.size)
                        }
                        .buttonStyle(.glassProminent)
                        .buttonBorderShape(.circle)
                        .tint(.accentColor)
                        .keyboardShortcut("n", modifiers: [])
                        .padding(AddButtonMetrics.margin)
                        .accessibilityLabel(button.title)
                        .accessibilityIdentifier(button.identifier)
                    }
                }
            }
        }
    }

    /// The tab bar's bottom accessory: the "+" handed up by the screen in front, gone without one.
    private struct AddButtonAccessory: ViewModifier {
        @State private var button: AddButton?

        func body(content: Content) -> some View {
            content
                .onPreferenceChange(AddButtonKey.self) { button = $0 }
                .tabViewBottomAccessory(isEnabled: button != nil) {
                    if let button {
                        Button(button.title, systemImage: "plus", action: button.action)
                            .fontWeight(.semibold)
                            .frame(maxWidth: .infinity, maxHeight: .infinity)
                            .contentShape(Rectangle())
                            .keyboardShortcut("n", modifiers: [])
                            .accessibilityIdentifier(button.identifier)
                    }
                }
        }
    }
#endif
