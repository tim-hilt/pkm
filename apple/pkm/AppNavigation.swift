import Foundation
import Observation

/// Where the user is in the app: the selected Context or Settings, and whether
/// an Item of it fills the window. The selected Context is the last one,
/// remembered per Device; Tasks on the first launch.
@MainActor @Observable
final class AppNavigation {
    /// The Context shown, unless `isShowingSettings`; changing it is remembered
    /// and leaves Settings.
    var selected: AppContext {
        didSet {
            defaults?.set(selected.rawValue, forKey: Self.lastContextKey)
            isShowingSettings = false
        }
    }

    /// A sidebar row: a Context, or on iPhone and iPad Settings.
    enum Destination: Hashable {
        case context(AppContext)
        case settings
    }

    /// What the detail column shows. Settings is not remembered as the last Context.
    var destination: Destination {
        get { isShowingSettings ? .settings : .context(selected) }
        set {
            switch newValue {
            case .context(let context): selected = context
            case .settings: isShowingSettings = true
            }
        }
    }

    /// Whether an Item (an open Drawing) fills the window.
    var isItemOpen = false

    /// Whether Settings is shown instead of the Context (iPhone and iPad).
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

    /// Follows a `pkm://<context>[/<id>]` link: switches to the Context and
    /// returns the Item ID to open there, if any. Other URLs and unknown
    /// Contexts are ignored (`nil` and nothing changes).
    func follow(_ url: URL) -> Link? {
        guard url.scheme == "pkm", let host = url.host(), let context = AppContext(rawValue: host) else { return nil }
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
