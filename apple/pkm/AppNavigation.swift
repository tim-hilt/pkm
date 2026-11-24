import Foundation
import Observation

/// Where the user is in the app: the selected Context, whether Settings is
/// open over it, and whether an Item of it fills the window. The selected Context is the last one,
/// remembered per Device; Tasks on the first launch.
@MainActor @Observable
final class AppNavigation {
    /// The Context shown; changing it is remembered.
    var selected: AppContext {
        didSet { defaults?.set(selected.rawValue, forKey: Self.lastContextKey) }
    }

    /// Whether an Item (an open Drawing) fills the window.
    var isItemOpen = false

    /// Whether the Settings sheet is open over the Context (iPhone and iPad).
    /// Settings is not remembered as the last Context.
    var isShowingSettings = false

    /// Focused work is not left by accident: no switching while an Item is open.
    var canSwitchContext: Bool { !isItemOpen }

    @ObservationIgnored private let defaults: UserDefaults?
    private static let lastContextKey = "lastContext"

    /// `defaults` holds the last Context; `nil` remembers nothing.
    init(remembering defaults: UserDefaults? = .standard) {
        self.defaults = defaults
        selected = defaults?.string(forKey: Self.lastContextKey).flatMap(AppContext.init(rawValue:)) ?? .tasks
    }

    /// Follows a `pkm://<context>[/<id>]` link: switches to the Context (closing
    /// Settings) and returns the Item ID to open there, if any. Other URLs and unknown
    /// Contexts are ignored (`nil` and nothing changes).
    func follow(_ url: URL) -> Link? {
        guard url.scheme == "pkm", let host = url.host(), let context = AppContext(rawValue: host) else { return nil }
        isShowingSettings = false
        selected = context
        let id = url.pathComponents.dropFirst().first
        return Link(context: context, itemID: id)
    }

    /// A followed link: the Context now shown and the Item asked for.
    struct Link: Equatable {
        let context: AppContext
        let itemID: String?
    }
}
