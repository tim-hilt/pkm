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
/// `service`. Never synced. Losing any part means not Connected.
struct CloudCredentials {
    let defaults: UserDefaults
    let service: String

    private static let urlKey = "cloud.webdav.url"
    private static let usernameKey = "cloud.webdav.username"
    private static let passwordName = "WebDAV password"

    /// What `save` kept; nil if there is none or a part is missing.
    func load() -> WebDAVAccount? {
        guard let url = defaults.string(forKey: Self.urlKey),
            let username = defaults.string(forKey: Self.usernameKey),
            let password = secret()
        else { return nil }
        return WebDAVAccount(url: url, username: username, password: password)
    }

    /// Replaces what was saved. False if the Keychain refused the password;
    /// then nothing is saved.
    @discardableResult
    func save(_ account: WebDAVAccount) -> Bool {
        forget()
        guard store(account.password) else { return false }
        defaults.set(account.url, forKey: Self.urlKey)
        defaults.set(account.username, forKey: Self.usernameKey)
        return true
    }

    /// Deletes everything saved.
    func forget() {
        defaults.removeObject(forKey: Self.urlKey)
        defaults.removeObject(forKey: Self.usernameKey)
        SecItemDelete(query() as CFDictionary)
    }

    private func store(_ secret: String) -> Bool {
        var item = query()
        item[kSecValueData] = Data(secret.utf8)
        item[kSecAttrAccessible] = kSecAttrAccessibleAfterFirstUnlock
        return SecItemAdd(item as CFDictionary, nil) == errSecSuccess
    }

    private func secret() -> String? {
        var item = query()
        item[kSecReturnData] = true
        var result: CFTypeRef?
        guard SecItemCopyMatching(item as CFDictionary, &result) == errSecSuccess, let data = result as? Data
        else { return nil }
        return String(data: data, encoding: .utf8)
    }

    private func query() -> [CFString: Any] {
        [kSecClass: kSecClassGenericPassword, kSecAttrService: service, kSecAttrAccount: Self.passwordName]
    }
}

extension CloudCredentials {
    /// The app's credentials, in the standard defaults and the Keychain.
    static var app: CloudCredentials {
        CloudCredentials(defaults: .standard, service: "com.timhilt.excalidrawnative.cloud")
    }
}
