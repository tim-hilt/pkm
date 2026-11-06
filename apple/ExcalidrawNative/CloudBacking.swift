import Foundation
import SwiftUI

/// Where Drawings can be backed up. Only WebDAV can be Connected so far; the
/// picker shows the others, disabled.
enum CloudProvider: CaseIterable, Hashable {
    case webDAV
    case iCloudDrive
    case googleDrive
    case oneDrive

    var name: String {
        switch self {
        case .webDAV: "WebDAV"
        case .iCloudDrive: "iCloud Drive"
        case .googleDrive: "Google Drive"
        case .oneDrive: "OneDrive"
        }
    }

    /// Whether the app can Connect it yet.
    var isAvailable: Bool { self == .webDAV }

    /// What Settings calls the detail of a Connection to it.
    var detailLabel: String {
        switch self {
        case .webDAV: "Server"
        case .iCloudDrive: "Folder"
        case .googleDrive, .oneDrive: "Account"
        }
    }

    /// The icon shown next to its name, 20pt high: a symbol follows the text,
    /// a logo is drawn unmodified.
    @ViewBuilder var icon: some View {
        switch self {
        case .webDAV:
            Image(systemName: "server.rack").resizable().scaledToFit().frame(height: 20)
        case .iCloudDrive: logo("ICloudDrive")
        case .googleDrive: logo("GoogleDrive")
        case .oneDrive: logo("OneDrive")
        }
    }

    private func logo(_ name: String) -> some View {
        Image(name).renderingMode(.original).resizable().scaledToFit().frame(height: 20)
    }
}

/// The Cloud Provider this Device is Connected to.
struct CloudConnection: Equatable {
    let provider: CloudProvider
    /// What Settings shows of it, labelled `provider.detailLabel`: the server's URL.
    let detail: String
}

extension ConnectError {
    /// What Settings says next to the form.
    var message: String {
        switch self {
        case .WrongCredentials: "Wrong username or password"
        case .Unreachable: "Server not reachable, or its certificate is not trusted"
        case .NotHttps: "URL must start with https://, so your password never travels in plain text"
        case .NotWebDav: "Not a WebDAV server"
        }
    }
}

/// The Keychain refused to keep the password.
struct CredentialsNotSaved: LocalizedError {
    var errorDescription: String? { "The password could not be saved to the Keychain" }
}

/// Cloud Backing: the Connection to the Cloud Provider this Device is Connected
/// to, and its credentials. Nothing Syncs yet. One per app.
@MainActor @Observable
final class CloudBackingModel {
    /// The Cloud Provider Connected to; nil while Cloud Backing is off.
    private(set) var connection: CloudConnection?

    @ObservationIgnored private let credentials: CloudCredentials
    @ObservationIgnored private let checkWebDAV: @Sendable (String, String, String) throws -> Void

    /// Cloud Backing keeping its credentials in `credentials`. The Connection
    /// saved there is shown again without contacting the server, so launching
    /// offline works. `checkWebDAV` is the core's Connect; tests replace it.
    init(
        credentials: CloudCredentials = .app,
        checkWebDAV: @escaping @Sendable (String, String, String) throws -> Void = {
            try connectWebdav(url: $0, username: $1, password: $2)
        }
    ) {
        self.credentials = credentials
        self.checkWebDAV = checkWebDAV
        if let account = credentials.load() {
            connection = CloudConnection(provider: .webDAV, detail: account.url)
        }
    }

    /// Connects to the WebDAV server at `url`: checks it and creates the
    /// `ExcalidrawNative` folder. Throws a `ConnectError` if that fails; then
    /// nothing is kept. Keeps the credentials, the password in the Keychain,
    /// until `disconnect`. The core blocks while it talks to the server, so
    /// this runs it off the main actor.
    func connect(url: String, username: String, password: String) async throws {
        let check = checkWebDAV
        try await Task.detached { try check(url, username, password) }.value
        let account = WebDAVAccount(
            url: url.trimmingCharacters(in: .whitespaces), username: username, password: password)
        guard credentials.save(account) else { throw CredentialsNotSaved() }
        connection = CloudConnection(provider: .webDAV, detail: account.url)
    }

    /// Forgets the credentials on this Device only: the cloud and every
    /// Drawing stay as they are.
    func disconnect() {
        credentials.forget()
        connection = nil
    }
}
