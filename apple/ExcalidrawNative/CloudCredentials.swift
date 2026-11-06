import Foundation
import Security

/// The details of a Connected WebDAV server.
struct WebDAVAccount: Equatable, Sendable {
    let url: String
    let username: String
    let password: String
}

/// Where the credentials of the Connected Cloud Provider are kept on this
/// Device: URL and username in `defaults`, the password in the Keychain under
/// `service`; for iCloud Drive the bookmark of the grant in `defaults`; for
/// Google Drive the account's email in `defaults` and its refresh token in the
/// Keychain. Never synced. Losing any part means not Connected.
// `UserDefaults` and the Keychain are safe to use from any thread.
struct CloudCredentials: @unchecked Sendable {
    let defaults: UserDefaults
    let service: String

    private static let urlKey = "cloud.webdav.url"
    private static let usernameKey = "cloud.webdav.username"
    private static let bookmarkKey = "cloud.icloud.bookmark"
    private static let googleEmailKey = "cloud.google.email"
    private static let passwordName = "WebDAV password"
    private static let googleTokenName = "Google Drive refresh token"

    /// What `save` kept; nil if there is none or a part is missing.
    func load() -> WebDAVAccount? {
        guard let url = defaults.string(forKey: Self.urlKey),
            let username = defaults.string(forKey: Self.usernameKey),
            let password = secret()
        else { return nil }
        return WebDAVAccount(url: url, username: username, password: password)
    }

    /// The security-scoped bookmark of the iCloud Drive grant, if iCloud Drive
    /// is Connected.
    func loadBookmark() -> Data? { defaults.data(forKey: Self.bookmarkKey) }

    /// The Google account `saveGoogle` kept; nil if there is none or a part is missing.
    func loadGoogle() -> GoogleDriveAccount? {
        guard let email = defaults.string(forKey: Self.googleEmailKey),
            let token = secret(Self.googleTokenName)
        else { return nil }
        return GoogleDriveAccount(refreshToken: token, email: email)
    }

    /// Replaces what was saved with the Google account. False if the Keychain
    /// refused the token; then nothing is saved.
    @discardableResult
    func saveGoogle(_ account: GoogleDriveAccount) -> Bool {
        forget()
        guard store(account.refreshToken, as: Self.googleTokenName) else { return false }
        defaults.set(account.email, forKey: Self.googleEmailKey)
        return true
    }

    /// Keeps `token`, which replaced the account's refresh token. Called on
    /// the Sync loop's thread.
    func rotateGoogleToken(to token: String) {
        guard defaults.string(forKey: Self.googleEmailKey) != nil else { return }
        SecItemDelete(query(Self.googleTokenName) as CFDictionary)
        _ = store(token, as: Self.googleTokenName)
    }

    /// Replaces what was saved with the iCloud Drive grant `bookmark`.
    func saveBookmark(_ bookmark: Data) {
        forget()
        defaults.set(bookmark, forKey: Self.bookmarkKey)
    }

    /// Replaces what was saved. False if the Keychain refused the password;
    /// then nothing is saved.
    @discardableResult
    func save(_ account: WebDAVAccount) -> Bool {
        forget()
        guard store(account.password, as: Self.passwordName) else { return false }
        defaults.set(account.url, forKey: Self.urlKey)
        defaults.set(account.username, forKey: Self.usernameKey)
        return true
    }

    /// Deletes everything saved.
    func forget() {
        defaults.removeObject(forKey: Self.urlKey)
        defaults.removeObject(forKey: Self.usernameKey)
        defaults.removeObject(forKey: Self.bookmarkKey)
        defaults.removeObject(forKey: Self.googleEmailKey)
        SecItemDelete(query(Self.passwordName) as CFDictionary)
        SecItemDelete(query(Self.googleTokenName) as CFDictionary)
    }

    private func store(_ secret: String, as name: String) -> Bool {
        var item = query(name)
        item[kSecValueData] = Data(secret.utf8)
        item[kSecAttrAccessible] = kSecAttrAccessibleAfterFirstUnlock
        return SecItemAdd(item as CFDictionary, nil) == errSecSuccess
    }

    private func secret(_ name: String = CloudCredentials.passwordName) -> String? {
        var item = query(name)
        item[kSecReturnData] = true
        var result: CFTypeRef?
        guard SecItemCopyMatching(item as CFDictionary, &result) == errSecSuccess, let data = result as? Data
        else { return nil }
        return String(data: data, encoding: .utf8)
    }

    private func query(_ name: String) -> [CFString: Any] {
        [kSecClass: kSecClassGenericPassword, kSecAttrService: service, kSecAttrAccount: name]
    }
}

extension CloudCredentials {
    /// The app's credentials, in the standard defaults and the Keychain.
    static var app: CloudCredentials {
        CloudCredentials(defaults: .standard, service: "com.timhilt.excalidrawnative.cloud")
    }
}
