import Foundation

extension UserDefaults {
    /// Empty settings of the test's own: tests must not read or write the
    /// user's real ones (a stored Pencil-only choice makes finger taps draw
    /// nothing), nor each other's, as suites run in parallel. A suite name that
    /// is an absolute path keeps the plist at that path, here in the temporary
    /// directory, instead of adding one file per test to the app's
    /// Library/Preferences on every run.
    static func isolated() -> UserDefaults {
        let path = FileManager.default.temporaryDirectory
            .appending(path: "workingtitle.tests.\(UUID().uuidString)").path
        return UserDefaults(suiteName: path)!
    }
}
