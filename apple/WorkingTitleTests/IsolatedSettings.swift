import Foundation

extension UserDefaults {
    /// Empty settings that outlive nothing: tests must not read or write the
    /// user's real ones (a stored Pencil-only choice makes finger taps draw nothing).
    static func isolated() -> UserDefaults {
        let name = "workingtitle.tests.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: name)!
        defaults.removePersistentDomain(forName: name)
        return defaults
    }
}
