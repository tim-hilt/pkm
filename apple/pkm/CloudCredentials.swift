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
/// Google Drive and OneDrive the account's email in `defaults` and its refresh
/// token in the Keychain. Never synced. Losing any part means not Connected.
// `UserDefaults` and the Keychain are safe to use from any thread.
struct CloudCredentials: @unchecked Sendable {
    let defaults: UserDefaults
    let service: String

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
            let password = secret()
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

    /// Replaces what was saved with the Google account. False if the Keychain
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
        guard let email = defaults.string(forKey: slot.emailKey), let token = secret(slot.tokenName)
        else { return nil }
        return (token, email)
    }

    private func saveOAuth(_ slot: OAuthSlot, token: String, email: String) -> Bool {
        forget()
        guard store(token, as: slot.tokenName) else { return false }
        defaults.set(email, forKey: slot.emailKey)
        return true
    }

    private func rotateOAuth(_ slot: OAuthSlot, to token: String) {
        guard defaults.string(forKey: slot.emailKey) != nil else { return }
        SecItemDelete(query(slot.tokenName) as CFDictionary)
        _ = store(token, as: slot.tokenName)
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
        SecItemDelete(query(Self.passwordName) as CFDictionary)
        for slot in OAuthSlot.all {
            defaults.removeObject(forKey: slot.emailKey)
            SecItemDelete(query(slot.tokenName) as CFDictionary)
        }
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
        CloudCredentials(defaults: .standard, service: "com.timhilt.pkm.cloud")
    }
}
