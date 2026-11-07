import SwiftUI

#if os(macOS)
    import AppKit
#else
    import UIKit
#endif

/// The user's choice of appearance: follow the system, or always Light or
/// always Dark. It sets the appearance of the whole app (window, sheets,
/// Settings), so everything that follows the appearance (Dark mode on the
/// Canvas, chrome colours, thumbnails) follows the choice with no code of its
/// own. Like Dark mode, it never changes a Drawing or an export (ADR-0010).
enum Appearance: String, CaseIterable {
    case system, light, dark

    /// Where it is kept in the settings.
    static let key = "appearance"

    /// The stored choice; System when none (or an unknown one) is stored.
    static func stored(in settings: UserDefaults = .standard) -> Appearance {
        settings.string(forKey: key).flatMap(Appearance.init(rawValue:)) ?? .system
    }

    var title: LocalizedStringResource {
        switch self {
        case .system: "System"
        case .light: "Light"
        case .dark: "Dark"
        }
    }

    #if os(macOS)
        /// For `NSApp.appearance`: nil follows the system.
        var nsAppearance: NSAppearance? {
            switch self {
            case .system: nil
            case .light: NSAppearance(named: .aqua)
            case .dark: NSAppearance(named: .darkAqua)
            }
        }
    #else
        /// For the window's `overrideUserInterfaceStyle`; it reaches the sheets
        /// the window presents.
        var userInterfaceStyle: UIUserInterfaceStyle {
            switch self {
            case .system: .unspecified
            case .light: .light
            case .dark: .dark
            }
        }
    #endif
}

/// The Appearance picker, for Settings and the Mac's View menu.
struct AppearancePicker: View {
    @AppStorage(Appearance.key) private var appearance = Appearance.system

    var body: some View {
        Picker("Appearance", selection: $appearance) {
            ForEach(Appearance.allCases, id: \.self) { Text($0.title).tag($0) }
        }
        .accessibilityIdentifier("appearance")
    }
}

#if os(macOS)
    /// View > Appearance on the Mac: System, Light or Dark.
    struct AppearanceCommands: Commands {
        var body: some Commands {
            CommandGroup(after: .toolbar) {
                Divider()
                AppearancePicker()
            }
        }
    }
#endif

extension View {
    /// Applies the stored Appearance to the app, now and whenever it changes.
    func appliesAppearance() -> some View { modifier(AppliesAppearance()) }
}

private struct AppliesAppearance: ViewModifier {
    @AppStorage(Appearance.key) private var appearance = Appearance.system

    func body(content: Content) -> some View {
        #if os(macOS)
            // The app's appearance reaches every window, Settings included.
            content.onChange(of: appearance, initial: true) { NSApp.appearance = appearance.nsAppearance }
        #else
            content.background(WindowStyle(style: appearance.userInterfaceStyle))
        #endif
    }
}

#if !os(macOS)
    /// Sets the style of the window it is in, as soon as it is in one.
    private struct WindowStyle: UIViewRepresentable {
        let style: UIUserInterfaceStyle

        func makeUIView(context: Context) -> StyledView {
            let view = StyledView()
            view.isUserInteractionEnabled = false
            view.style = style
            return view
        }

        func updateUIView(_ view: StyledView, context: Context) { view.style = style }

        final class StyledView: UIView {
            var style = UIUserInterfaceStyle.unspecified {
                didSet { window?.overrideUserInterfaceStyle = style }
            }

            override func didMoveToWindow() {
                super.didMoveToWindow()
                window?.overrideUserInterfaceStyle = style
            }
        }
    }
#endif
