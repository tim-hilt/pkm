import Foundation
import Network
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

/// How many of the Drawings a Sync step transfers are done.
struct SyncProgress: Equatable {
    let done: Int
    let total: Int
}

/// Hears from the core's Sync loop, on a thread of its own, what it does, and
/// hands that on to the main actor.
private final class SyncEvents: SyncObserver, @unchecked Sendable {
    private let onChanged: @MainActor @Sendable ([String]) -> Void
    private let onProgress: @MainActor @Sendable (SyncProgress?) -> Void
    private let onStatus: @MainActor @Sendable (SyncStatus) -> Void

    init(
        onChanged: @escaping @MainActor @Sendable ([String]) -> Void,
        onProgress: @escaping @MainActor @Sendable (SyncProgress?) -> Void,
        onStatus: @escaping @MainActor @Sendable (SyncStatus) -> Void
    ) {
        (self.onChanged, self.onProgress, self.onStatus) = (onChanged, onProgress, onStatus)
    }

    func changed(names: [String]) {
        Task { @MainActor [onChanged] in onChanged(names) }
    }

    func progress(done: UInt32, total: UInt32) {
        let progress = total == 0 ? nil : SyncProgress(done: Int(done), total: Int(total))
        Task { @MainActor [onProgress] in onProgress(progress) }
    }

    func status(status: SyncStatus) {
        Task { @MainActor [onStatus] in onStatus(status) }
    }
}

/// Tells whether the Device has a connection.
private final class NetworkMonitor: @unchecked Sendable {
    private let monitor = NWPathMonitor()

    init(_ handler: @escaping @MainActor @Sendable (Bool) -> Void) {
        monitor.pathUpdateHandler = { path in
            let online = path.status == .satisfied
            Task { @MainActor in handler(online) }
        }
        monitor.start(queue: DispatchQueue(label: "cloud-backing.network"))
    }

    deinit { monitor.cancel() }
}

/// Cloud Backing: the Connection to the Cloud Provider this Device is Connected
/// to, its credentials, and the Sync of the Drawings with it while Connected.
/// One per app.
@MainActor @Observable
final class CloudBackingModel {
    /// The Cloud Provider Connected to; nil while Cloud Backing is off.
    private(set) var connection: CloudConnection?

    /// What Sync is doing; nil until the first step is through.
    private(set) var status: SyncStatus?
    /// Drawings done of all Drawings a running Sync step transfers.
    private(set) var progress: SyncProgress?
    /// When a Sync step last went through.
    private(set) var lastSync: Date?
    /// The Cloud Provider the user picked while Connected to another: asks
    /// to confirm before `confirmSwitch` Disconnects.
    private(set) var pendingSwitch: CloudProvider?

    /// What Settings says of `status`.
    var statusText: String {
        switch status {
        case nil: String(localized: "Not synced yet")
        case .upToDate: String(localized: "Up to date")
        case .syncing:
            progress.map { String(localized: "Syncing \($0.done) of \($0.total) Drawings") }
                ?? String(localized: "Syncing")
        case .offline: String(localized: "Offline")
        case .accessLost: String(localized: "Access lost")
        case .full: String(localized: "\(connection?.provider.name ?? "") is full")
        }
    }

    /// Whether Sync is blocked until the user acts, which the Gallery shows
    /// with a warning icon: the cloud rejected the credentials or is full.
    var hasProblem: Bool { status == .accessLost || status == .full }

    @ObservationIgnored private let credentials: CloudCredentials
    @ObservationIgnored private let checkWebDAV: @Sendable (String, String, String) throws -> Void

    /// Where Drawings are Synced, once the Gallery says so (`startSyncing`).
    private struct SyncTarget {
        let drawings: URL
        /// What was Synced, outside the Drawings folder; never Synced itself.
        let stateDirectory: URL
        let onChanged: @MainActor @Sendable ([String]) -> Void
    }
    @ObservationIgnored private var target: SyncTarget?
    /// The running Sync; nil while not Connected or not attached.
    @ObservationIgnored private var sync: CloudSync?
    @ObservationIgnored private var isInFront = true
    @ObservationIgnored private var isOnline = true
    @ObservationIgnored private var network: NetworkMonitor?

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
        // What was Synced with another server says nothing about this one.
        if let old = connection, old.detail != account.url { clearSyncState() }
        connection = CloudConnection(provider: .webDAV, detail: account.url)
        restartSync()
    }

    /// Syncs the Drawings in `drawings` while Connected, and calls `onChanged`
    /// (on the main actor) with the names of those Sync wrote or deleted here.
    /// `stateDirectory` holds what Sync remembers: not in `drawings`.
    func startSyncing(
        drawings: URL, stateDirectory: URL, onChanged: @escaping @MainActor @Sendable ([String]) -> Void
    ) {
        target = SyncTarget(drawings: drawings, stateDirectory: stateDirectory, onChanged: onChanged)
        network = NetworkMonitor { [weak self] online in self?.networkChanged(online: online) }
        restartSync()
    }

    /// The app came to the front (Sync at once) or went to the background
    /// (nothing runs).
    func appBecame(inFront: Bool) {
        isInFront = inFront
        if inFront { sync?.cameToFront() } else { sync?.wentToBackground() }
    }

    /// A Drawing changed on this Device: Sync a few seconds from now.
    func drawingsChanged() { sync?.localChange() }

    private func networkChanged(online: Bool) {
        isOnline = online
        sync?.setOnline(online: online)
    }

    /// Sync tells how it is doing: see `status`, `progress`, `lastSync`.
    private func apply(status: SyncStatus) {
        self.status = status
        if status == .upToDate || status == .full { lastSync = Date() }
    }

    private func restartSync() {
        sync = nil  // cancels the running step
        (status, progress) = (nil, nil)
        guard let target, let account = credentials.load() else { return }
        sync = try? CloudSync.startWebdav(
            url: account.url, username: account.username, password: account.password,
            drawingsDirectory: target.drawings.path(percentEncoded: false),
            stateDirectory: target.stateDirectory.path(percentEncoded: false),
            observer: SyncEvents(
                onChanged: target.onChanged,
                onProgress: { [weak self] in self?.progress = $0 },
                onStatus: { [weak self] in self?.apply(status: $0) }))
        if !isInFront { sync?.wentToBackground() }
        if !isOnline { sync?.setOnline(online: false) }
    }

    /// Reconnects after Sync lost access (the password changed): checks the
    /// server with the new `password`, which throws a `ConnectError` if it is
    /// still refused, and then lets Sync resume where it stopped.
    func reconnect(password: String) async throws {
        guard let account = credentials.load() else { return }
        let check = checkWebDAV
        try await Task.detached { try check(account.url, account.username, password) }.value
        let renewed = WebDAVAccount(url: account.url, username: account.username, password: password)
        guard credentials.save(renewed) else { throw CredentialsNotSaved() }
        if let sync {
            try? sync.reconnectWebdav(url: renewed.url, username: renewed.username, password: password)
        }
    }

    /// The user picked `provider` while Connected to another: to be confirmed.
    func requestSwitch(to provider: CloudProvider) {
        guard let connection, connection.provider != provider else { return }
        pendingSwitch = provider
    }

    /// The user confirmed: Disconnects the current Cloud Provider, so that
    /// the picked one can be Connected. Returns it.
    @discardableResult
    func confirmSwitch() -> CloudProvider? {
        guard let picked = pendingSwitch else { return nil }
        disconnect()
        return picked
    }

    func cancelSwitch() { pendingSwitch = nil }

    private func clearSyncState() {
        sync = nil
        if let target { forgetSyncState(stateDirectory: target.stateDirectory.path(percentEncoded: false)) }
    }

    /// Forgets the credentials on this Device only: the cloud and every
    /// Drawing stay as they are.
    func disconnect() {
        clearSyncState()
        credentials.forget()
        connection = nil
        (status, progress, lastSync, pendingSwitch) = (nil, nil, nil, nil)
    }
}
