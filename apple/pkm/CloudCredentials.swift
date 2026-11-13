import Foundation
import Security
import Synchronization

/// The details of a Connected WebDAV server.
struct WebDAVAccount: Equatable, Sendable {
    let url: String
    let username: String
    let password: String
}

/// Where the credentials of the Connected Cloud Provider are kept on this
/// Device: URL and username in `defaults`, the password in `secrets`; for
/// iCloud Drive the bookmark of the grant in `defaults`; for Google Drive and
/// OneDrive the account's email in `defaults` and its refresh token in
/// `secrets`. Never synced. Losing any part means not Connected.
// `UserDefaults` is safe to use from any thread.
struct CloudCredentials: @unchecked Sendable {
    let defaults: UserDefaults
    let secrets: any SecretStore

    private static let urlKey = "cloud.webdav.url"
    private static let usernameKey = "cloud.webdav.username"
    private static let bookmarkKey = "cloud.icloud.bookmark"
    private static let passwordName = "WebDAV password"

    /// Where the account of a Cloud Provider that signs in with OAuth is kept.
    private struct OAuthSlot {
        let emailKey: String
        let tokenName: String
        static let google = OAuthSlot(emailKey: "cloud.google.email", tokenName: "Google Drive refresh token")
        static let oneDrive = OAuthSlot(emailKey: "cloud.onedrive.email", tokenName: "OneDrive refresh token")
        static let all = [google, oneDrive]
    }

    /// What `save` kept; nil if there is none or a part is missing.
    func load() -> WebDAVAccount? {
        guard let url = defaults.string(forKey: Self.urlKey),
            let username = defaults.string(forKey: Self.usernameKey),
            let password = secrets.read(Self.passwordName)
        else { return nil }
        return WebDAVAccount(url: url, username: username, password: password)
    }

    /// The security-scoped bookmark of the iCloud Drive grant, if iCloud Drive
    /// is Connected.
    func loadBookmark() -> Data? { defaults.data(forKey: Self.bookmarkKey) }

    /// The Google account `saveGoogle` kept; nil if there is none or a part is missing.
    func loadGoogle() -> GoogleDriveAccount? {
        loadOAuth(.google).map { GoogleDriveAccount(refreshToken: $0.token, email: $0.email) }
    }

    /// Replaces what was saved with the Google account. False if `secrets`
    /// refused the token; then nothing is saved.
    @discardableResult
    func saveGoogle(_ account: GoogleDriveAccount) -> Bool {
        saveOAuth(.google, token: account.refreshToken, email: account.email)
    }

    /// Keeps `token`, which replaced the account's refresh token. Called on
    /// the Sync loop's thread.
    func rotateGoogleToken(to token: String) { rotateOAuth(.google, to: token) }

    /// The OneDrive account `saveOneDrive` kept; nil if there is none or a part is missing.
    func loadOneDrive() -> OneDriveAccount? {
        loadOAuth(.oneDrive).map { OneDriveAccount(refreshToken: $0.token, email: $0.email) }
    }

    /// Like `saveGoogle`, for OneDrive.
    @discardableResult
    func saveOneDrive(_ account: OneDriveAccount) -> Bool {
        saveOAuth(.oneDrive, token: account.refreshToken, email: account.email)
    }

    /// Like `rotateGoogleToken`, for OneDrive.
    func rotateOneDriveToken(to token: String) { rotateOAuth(.oneDrive, to: token) }

    private func loadOAuth(_ slot: OAuthSlot) -> (token: String, email: String)? {
        guard let email = defaults.string(forKey: slot.emailKey), let token = secrets.read(slot.tokenName)
        else { return nil }
        return (token, email)
    }

    private func saveOAuth(_ slot: OAuthSlot, token: String, email: String) -> Bool {
        forget()
        guard secrets.write(token, as: slot.tokenName) else { return false }
        defaults.set(email, forKey: slot.emailKey)
        return true
    }

    private func rotateOAuth(_ slot: OAuthSlot, to token: String) {
        guard defaults.string(forKey: slot.emailKey) != nil else { return }
        _ = secrets.write(token, as: slot.tokenName)
    }

    /// Replaces what was saved with the iCloud Drive grant `bookmark`.
    func saveBookmark(_ bookmark: Data) {
        forget()
        defaults.set(bookmark, forKey: Self.bookmarkKey)
    }

    /// Replaces what was saved. False if `secrets` refused the password;
    /// then nothing is saved.
    @discardableResult
    func save(_ account: WebDAVAccount) -> Bool {
        forget()
        guard secrets.write(account.password, as: Self.passwordName) else { return false }
        defaults.set(account.url, forKey: Self.urlKey)
        defaults.set(account.username, forKey: Self.usernameKey)
        return true
    }

    /// Deletes everything saved.
    func forget() {
        defaults.removeObject(forKey: Self.urlKey)
        defaults.removeObject(forKey: Self.usernameKey)
        defaults.removeObject(forKey: Self.bookmarkKey)
        secrets.delete(Self.passwordName)
        for slot in OAuthSlot.all {
            defaults.removeObject(forKey: slot.emailKey)
            secrets.delete(slot.tokenName)
        }
    }
}

extension CloudCredentials {
    /// The credentials for this launch: the app's, in the standard defaults and
    /// the Keychain. Launches for UI tests and screenshots (`-UITesting…`) and
    /// as the host of unit tests get empty ones in memory instead: they must
    /// not see the user's Connection, and reading the Keychain from a rebuilt
    /// Debug build makes macOS ask for access.
    static func forLaunch(
        arguments: [String] = ProcessInfo.processInfo.arguments,
        environment: [String: String] = ProcessInfo.processInfo.environment
    ) -> CloudCredentials {
        let isolated =
            arguments.contains { $0.hasPrefix("-UITesting") } || environment["XCTestConfigurationFilePath"] != nil
        guard isolated else {
            return CloudCredentials(defaults: .standard, secrets: KeychainSecrets(service: "com.timhilt.pkm.cloud"))
        }
        let path = FileManager.default.temporaryDirectory.appending(path: "pkm.cloud.\(UUID().uuidString)").path
        return CloudCredentials(defaults: UserDefaults(suiteName: path)!, secrets: MemorySecrets())
    }
}

/// Where `CloudCredentials` keeps passwords and tokens, by name.
protocol SecretStore: Sendable {
    func read(_ name: String) -> String?
    /// Replaces the secret named `name`. False if it could not be kept.
    func write(_ secret: String, as name: String) -> Bool
    func delete(_ name: String)
}

/// The Keychain's generic passwords of `service`, readable after the first
/// unlock. The Keychain is safe to use from any thread.
struct KeychainSecrets: SecretStore {
    let service: String

    func read(_ name: String) -> String? {
        var item = query(name)
        item[kSecReturnData] = true
        var result: CFTypeRef?
        guard SecItemCopyMatching(item as CFDictionary, &result) == errSecSuccess, let data = result as? Data
        else { return nil }
        return String(data: data, encoding: .utf8)
    }

    func write(_ secret: String, as name: String) -> Bool {
        delete(name)
        var item = query(name)
        item[kSecValueData] = Data(secret.utf8)
        item[kSecAttrAccessible] = kSecAttrAccessibleAfterFirstUnlock
        return SecItemAdd(item as CFDictionary, nil) == errSecSuccess
    }

    func delete(_ name: String) {
        SecItemDelete(query(name) as CFDictionary)
    }

    private func query(_ name: String) -> [CFString: Any] {
        [kSecClass: kSecClassGenericPassword, kSecAttrService: service, kSecAttrAccount: name]
    }
}

/// Secrets gone at quit, for tests, UI tests and screenshots.
final class MemorySecrets: SecretStore {
    private let secrets = Mutex<[String: String]>([:])

    func read(_ name: String) -> String? { secrets.withLock { $0[name] } }

    func write(_ secret: String, as name: String) -> Bool {
        secrets.withLock { $0[name] = secret }
        return true
    }

    func delete(_ name: String) { secrets.withLock { _ = $0.removeValue(forKey: name) } }
}
