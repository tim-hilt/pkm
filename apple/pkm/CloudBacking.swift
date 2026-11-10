import Foundation
import Network
import SwiftUI

/// Where Drawings can be backed up; the picker shows all of them.
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
        case .ICloudDriveUnavailable: "Turn on iCloud Drive in Settings"
        // Without the paid iCloud capability the app can't tell whether iCloud
        // Drive is on (`ubiquityIdentityToken` is always nil); when it is off,
        // the picker doesn't offer it.
        case .WrongFolder:
            "Pick iCloud Drive itself, not a folder in it, so your Drawings end up in iCloud Drive/pkm. If iCloud Drive is missing, turn it on in Settings."
        case .UnsupportedAccount: "Work and school accounts are not supported. Use a personal Microsoft account."
        case .AccessDenied: "Access denied"
        case .SignInFailed: "Sign-in failed. Try again."
        }
    }
}

/// The Keychain refused to keep the password or token.
struct CredentialsNotSaved: LocalizedError {
    var errorDescription: String? { "The password could not be saved to the Keychain" }
}

/// Hears from the core's Sync loop of a refresh token to keep in the Keychain.
private final class RotatedTokens: TokenObserver {
    private let keep: @Sendable (String) -> Void
    init(_ keep: @escaping @Sendable (String) -> Void) { self.keep = keep }
    func refreshTokenRotated(token: String) { keep(token) }

    static func google(_ credentials: CloudCredentials) -> RotatedTokens {
        RotatedTokens { credentials.rotateGoogleToken(to: $0) }
    }

    static func oneDrive(_ credentials: CloudCredentials) -> RotatedTokens {
        RotatedTokens { credentials.rotateOneDriveToken(to: $0) }
    }
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
    private let onBooksChanged: @MainActor @Sendable ([String]) -> Void
    private let onProgress: @MainActor @Sendable (SyncProgress?) -> Void
    private let onStatus: @MainActor @Sendable (SyncStatus) -> Void

    init(
        onChanged: @escaping @MainActor @Sendable ([String]) -> Void,
        onBooksChanged: @escaping @MainActor @Sendable ([String]) -> Void,
        onProgress: @escaping @MainActor @Sendable (SyncProgress?) -> Void,
        onStatus: @escaping @MainActor @Sendable (SyncStatus) -> Void
    ) {
        (self.onChanged, self.onBooksChanged, self.onProgress, self.onStatus) =
            (onChanged, onBooksChanged, onProgress, onStatus)
    }

    func changed(names: [String]) {
        Task { @MainActor [onChanged] in onChanged(names) }
    }

    func booksChanged(names: [String]) {
        Task { @MainActor [onBooksChanged] in onBooksChanged(names) }
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
    @ObservationIgnored private let showSignInPage: @MainActor (URL, String) async throws -> URL

    /// Where Drawings are Synced, once the Gallery says so (`startSyncing`).
    private struct SyncTarget {
        let drawings: URL
        let books: URL
        /// What was Synced, outside the Context folders; never Synced itself.
        let stateDirectory: URL
        let onChanged: @MainActor @Sendable ([String]) -> Void
        let onBooksChanged: @MainActor @Sendable ([String]) -> Void
    }
    @ObservationIgnored private var target: SyncTarget?
    /// The running Sync; nil while not Connected or not attached.
    @ObservationIgnored private var sync: CloudSync?
    /// iCloud Drive's root while Sync uses it, which needs its security scope.
    @ObservationIgnored private var scopedRoot: URL?
    @ObservationIgnored private var isInFront = true
    @ObservationIgnored private var isOnline = true
    @ObservationIgnored private var network: NetworkMonitor?

    /// Cloud Backing keeping its credentials in `credentials`. The Connection
    /// saved there is shown again without contacting the server, so launching
    /// offline works. `checkWebDAV` is the core's Connect and `showSignInPage`
    /// shows a provider's sign-in page and returns the redirect; tests replace them.
    init(
        credentials: CloudCredentials = .app,
        checkWebDAV: @escaping @Sendable (String, String, String) throws -> Void = {
            try connectWebdav(url: $0, username: $1, password: $2)
        },
        showSignInPage: @escaping @MainActor (URL, String) async throws -> URL = {
            try await pkm.showSignInPage(url: $0, callbackScheme: $1)
        }
    ) {
        self.credentials = credentials
        self.checkWebDAV = checkWebDAV
        self.showSignInPage = showSignInPage
        if let google = credentials.loadGoogle() {
            connection = CloudConnection(provider: .googleDrive, detail: google.email)
        } else if let oneDrive = credentials.loadOneDrive() {
            connection = CloudConnection(provider: .oneDrive, detail: oneDrive.email)
        } else if let account = credentials.load() {
            connection = CloudConnection(provider: .webDAV, detail: account.url)
        } else if credentials.loadBookmark() != nil {
            connection = Self.iCloudConnection
        }
    }

    private static let iCloudFolder = "pkm"
    private static let iCloudConnection = CloudConnection(provider: .iCloudDrive, detail: iCloudFolder)

    /// Connects iCloud Drive through `granted`, the folder the user picked
    /// (also to Reconnect after the grant was lost). Throws a `ConnectError`
    /// if it is not iCloud Drive's root; then nothing is kept. Creates
    /// `pkm` in it if missing and keeps the grant, as a bookmark,
    /// until `disconnect`.
    func connectICloudDrive(granted: URL) async throws {
        let scoped = granted.startAccessingSecurityScopedResource()
        defer { if scoped { granted.stopAccessingSecurityScopedResource() } }
        let path = granted.path(percentEncoded: false)
        try await Task.detached { _ = try connectIcloudDrive(grantedDirectory: path) }.value
        let bookmark = try ICloudGrant.bookmark(for: granted)
        try adopt(Self.iCloudConnection) {
            credentials.saveBookmark(bookmark)
            return true
        }
    }

    /// Shows `new` as Connected once `save` kept its credentials (throws
    /// `CredentialsNotSaved` if it could not) and Syncs with it. Sync carries
    /// on where it stopped if `new` is what was Connected already (a
    /// Reconnect), handed the new credentials by `resume` if given, else
    /// restarted. Otherwise what it remembered is forgotten: it is the history
    /// of another cloud (or of one whose credentials were lost), and would
    /// make Sync delete here every Drawing that this cloud lacks.
    private func adopt(
        _ new: CloudConnection, resume: ((CloudSync) -> Void)? = nil, save: () -> Bool
    ) throws {
        let reconnecting = connection == new
        guard save() else { throw CredentialsNotSaved() }
        if !reconnecting { clearSyncState() }
        connection = new
        if reconnecting, let sync, let resume { resume(sync) } else { restartSync() }
    }

    /// Shows the sign-in page at `url` and returns the redirect it ends with.
    /// Nil if the user closed the page, which is nothing to report.
    private func redirect(fromSignInPage url: String, callbackScheme: String) async throws -> URL? {
        guard let page = URL(string: url) else { throw ConnectError.SignInFailed }
        do {
            return try await showSignInPage(page, callbackScheme)
        } catch is CancellationError {
            return nil
        }
    }

    /// Signs in on Google's page. Nil if the user closed it. Throws a
    /// `ConnectError` otherwise (`AccessDenied` if consent was refused).
    private func signInToGoogle() async throws -> GoogleDriveAccount? {
        let signIn = try GoogleDriveSignIn.start(clientId: GoogleOAuth.clientID)
        guard let redirect = try await redirect(fromSignInPage: signIn.url(), callbackScheme: signIn.callbackScheme())
        else { return nil }
        return try await Task.detached { try signIn.finish(redirect: redirect.absoluteString) }.value
    }

    /// Signs in on Microsoft's page. Nil if the user closed it. Throws a
    /// `ConnectError` otherwise (`AccessDenied` if consent was refused,
    /// `UnsupportedAccount` for a work or school account).
    private func signInToOneDrive() async throws -> OneDriveAccount? {
        let signIn = try OneDriveSignIn.start(clientId: OneDriveOAuth.clientID)
        guard let redirect = try await redirect(fromSignInPage: signIn.url(), callbackScheme: signIn.callbackScheme())
        else { return nil }
        return try await Task.detached { try signIn.finish(redirect: redirect.absoluteString) }.value
    }

    /// Connects Google Drive: the user signs in on Google's page and
    /// `My Drive/pkm` is created if missing. Does nothing if the
    /// user closes the page; throws a `ConnectError` if it fails, and then
    /// nothing is kept. Keeps the refresh token in the Keychain until
    /// `disconnect`. Also Reconnects after Sync lost access (revoked on
    /// Google's side): with the same account Sync resumes where it stopped.
    func connectGoogleDrive() async throws {
        guard let account = try await signInToGoogle() else { return }
        let tokens = RotatedTokens.google(credentials)
        try adopt(
            CloudConnection(provider: .googleDrive, detail: account.email),
            resume: {
                $0.reconnectGoogleDrive(
                    clientId: GoogleOAuth.clientID, refreshToken: account.refreshToken, tokens: tokens)
            },
            save: { credentials.saveGoogle(account) })
    }

    /// Connects OneDrive: the user signs in on Microsoft's page, and Drawings
    /// go to `Apps/pkm`. Does nothing if the user closes the
    /// page; throws a `ConnectError` if it fails, and then nothing is kept.
    /// Keeps the refresh token in the Keychain until `disconnect`. Also
    /// Reconnects, as `connectGoogleDrive` does.
    func connectOneDrive() async throws {
        guard let account = try await signInToOneDrive() else { return }
        let tokens = RotatedTokens.oneDrive(credentials)
        try adopt(
            CloudConnection(provider: .oneDrive, detail: account.email),
            resume: {
                $0.reconnectOneDrive(
                    clientId: OneDriveOAuth.clientID, refreshToken: account.refreshToken, tokens: tokens)
            },
            save: { credentials.saveOneDrive(account) })
    }

    /// Connects to the WebDAV server at `url`: checks it and creates the
    /// `pkm` folder. Throws a `ConnectError` if that fails; then
    /// nothing is kept. Keeps the credentials, the password in the Keychain,
    /// until `disconnect`. The core blocks while it talks to the server, so
    /// this runs it off the main actor.
    func connect(url: String, username: String, password: String) async throws {
        let check = checkWebDAV
        try await Task.detached { try check(url, username, password) }.value
        let account = WebDAVAccount(
            url: url.trimmingCharacters(in: .whitespaces), username: username, password: password)
        try adopt(
            CloudConnection(provider: .webDAV, detail: account.url),
            resume: { try? $0.reconnectWebdav(url: account.url, username: username, password: password) },
            save: { credentials.save(account) })
    }

    /// Syncs the Drawings in `drawings` and the Books in `books` while
    /// Connected, calling `onChanged` and `onBooksChanged` (on the main actor)
    /// with the names of what Sync wrote or deleted here. `stateDirectory`
    /// holds what Sync remembers: not in the Context folders.
    func startSyncing(
        drawings: URL, books: URL, stateDirectory: URL,
        onChanged: @escaping @MainActor @Sendable ([String]) -> Void,
        onBooksChanged: @escaping @MainActor @Sendable ([String]) -> Void
    ) {
        target = SyncTarget(
            drawings: drawings, books: books, stateDirectory: stateDirectory,
            onChanged: onChanged, onBooksChanged: onBooksChanged)
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

    /// Stops Sync, cancelling a running step, and gives up iCloud Drive's scope.
    private func stopSync() {
        sync = nil
        scopedRoot?.stopAccessingSecurityScopedResource()
        scopedRoot = nil
    }

    private func restartSync() {
        stopSync()
        (status, progress) = (nil, nil)
        guard let target else { return }
        if let google = credentials.loadGoogle() {
            sync = startGoogleDriveSync(account: google, target: target)
        } else if let oneDrive = credentials.loadOneDrive() {
            sync = startOneDriveSync(account: oneDrive, target: target)
        } else if let bookmark = credentials.loadBookmark() {
            sync = startICloudDriveSync(bookmark: bookmark, target: target)
        } else if let account = credentials.load() {
            sync = startWebDAVSync(account: account, target: target)
        }
        if !isInFront { sync?.wentToBackground() }
        if !isOnline { sync?.setOnline(online: false) }
    }

    private func observer(for target: SyncTarget) -> SyncEvents {
        SyncEvents(
            onChanged: target.onChanged,
            onBooksChanged: target.onBooksChanged,
            onProgress: { [weak self] in self?.progress = $0 },
            onStatus: { [weak self] in self?.apply(status: $0) })
    }

    /// Syncs with iCloud Drive; without the grant, the status is `accessLost`.
    private func startICloudDriveSync(bookmark: Data, target: SyncTarget) -> CloudSync? {
        guard let grant = ICloudGrant.resolve(bookmark), grant.folder.startAccessingSecurityScopedResource()
        else {
            status = .accessLost
            return nil
        }
        scopedRoot = grant.folder
        if let renewed = grant.renewed { credentials.saveBookmark(renewed) }
        let sync = try? CloudSync.startIcloudDrive(
            folder: grant.folder.appending(path: Self.iCloudFolder).path(percentEncoded: false),
            downloader: ICloudDownloader(),
            drawingsDirectory: target.drawings.path(percentEncoded: false),
            booksDirectory: target.books.path(percentEncoded: false),
            stateDirectory: target.stateDirectory.path(percentEncoded: false),
            observer: observer(for: target))
        if sync == nil { status = .accessLost }
        return sync
    }

    private func startGoogleDriveSync(account: GoogleDriveAccount, target: SyncTarget) -> CloudSync? {
        try? CloudSync.startGoogleDrive(
            clientId: GoogleOAuth.clientID, refreshToken: account.refreshToken,
            tokens: RotatedTokens.google(credentials),
            drawingsDirectory: target.drawings.path(percentEncoded: false),
            booksDirectory: target.books.path(percentEncoded: false),
            stateDirectory: target.stateDirectory.path(percentEncoded: false),
            observer: observer(for: target))
    }

    private func startOneDriveSync(account: OneDriveAccount, target: SyncTarget) -> CloudSync? {
        try? CloudSync.startOneDrive(
            clientId: OneDriveOAuth.clientID, refreshToken: account.refreshToken,
            tokens: RotatedTokens.oneDrive(credentials),
            drawingsDirectory: target.drawings.path(percentEncoded: false),
            booksDirectory: target.books.path(percentEncoded: false),
            stateDirectory: target.stateDirectory.path(percentEncoded: false),
            observer: observer(for: target))
    }

    private func startWebDAVSync(account: WebDAVAccount, target: SyncTarget) -> CloudSync? {
        try? CloudSync.startWebdav(
            url: account.url, username: account.username, password: account.password,
            drawingsDirectory: target.drawings.path(percentEncoded: false),
            booksDirectory: target.books.path(percentEncoded: false),
            stateDirectory: target.stateDirectory.path(percentEncoded: false),
            observer: observer(for: target))
    }

    /// Reconnects after Sync lost access (the password changed): checks the
    /// server with the new `password`, which throws a `ConnectError` if it is
    /// still refused, and then lets Sync resume where it stopped.
    func reconnect(password: String) async throws {
        guard let account = credentials.load() else { return }
        try await connect(url: account.url, username: account.username, password: password)
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
        stopSync()
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
