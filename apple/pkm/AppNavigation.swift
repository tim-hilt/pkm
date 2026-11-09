import Foundation
import Observation

/// Where the user is in the app: the selected Context, whether an Item of it
/// fills the window, and Settings. The selected Context is the last one,
/// remembered per Device; Tasks on the first launch.
@MainActor @Observable
final class AppNavigation {
    /// The Context shown; changing it is remembered.
    var selected: AppContext {
        didSet { defaults?.set(selected.rawValue, forKey: Self.lastContextKey) }
    }

    /// Whether an Item (an open Drawing) fills the window.
    var isItemOpen = false

    /// Whether Settings is shown as a sheet (iPhone and iPad).
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
}
