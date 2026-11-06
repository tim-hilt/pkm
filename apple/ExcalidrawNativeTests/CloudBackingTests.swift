import Foundation
import Testing

@testable import ExcalidrawNative

/// Connecting a WebDAV server through `CloudBackingModel`. The core's check of
/// the server is replaced; the core itself is tested in `crates/cloud`.
@MainActor @Suite struct CloudBackingTests {
    private let name = UUID().uuidString

    private var credentials: CloudCredentials {
        CloudCredentials(defaults: UserDefaults(suiteName: name)!, service: "excalidraw-native-test-\(name)")
    }

    private func model(
        checking check: @escaping @Sendable (String, String, String) throws -> Void = { _, _, _ in },
        iCloudDriveOn: Bool = true
    ) -> CloudBackingModel {
        CloudBackingModel(credentials: credentials, checkWebDAV: check, isICloudDriveOn: { iCloudDriveOn })
    }

    /// A folder standing in for iCloud Drive's root, removed by the caller.
    private func fakeICloudRoot(named folder: String = "com~apple~CloudDocs") throws -> URL {
        let root = FileManager.default.temporaryDirectory
            .appending(path: UUID().uuidString).appending(path: folder)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        return root
    }

    @Test func startsNotConnected() {
        #expect(model().connection == nil)
    }

    @Test func connectingShowsTheServerAndKeepsTheCredentials() async throws {
        let cloud = model()

        try await cloud.connect(url: "https://dav.example.com/files", username: "me", password: "s3cret")

        #expect(cloud.connection == CloudConnection(provider: .webDAV, detail: "https://dav.example.com/files"))
        #expect(
            credentials.load()
                == WebDAVAccount(url: "https://dav.example.com/files", username: "me", password: "s3cret"))
        credentials.forget()
    }

    @Test func theConnectionSurvivesARelaunchWithoutContactingTheServer() async throws {
        try await model().connect(url: "https://dav.example.com", username: "me", password: "pw")

        let relaunched = model(checking: { _, _, _ in Issue.record("contacted the server") })

        #expect(relaunched.connection?.detail == "https://dav.example.com")
        credentials.forget()
    }

    @Test func aFailedConnectKeepsNothing() async {
        let cloud = model(checking: { _, _, _ in throw ConnectError.WrongCredentials })

        await #expect(throws: ConnectError.self) {
            try await cloud.connect(url: "https://dav.example.com", username: "me", password: "wrong")
        }

        #expect(cloud.connection == nil)
        #expect(credentials.load() == nil)
    }

    @Test func disconnectingForgetsTheCredentialsAndReturnsToNotConnected() async throws {
        let cloud = model()
        try await cloud.connect(url: "https://dav.example.com", username: "me", password: "pw")

        cloud.disconnect()

        #expect(cloud.connection == nil)
        #expect(credentials.load() == nil)
        #expect(model().connection == nil)
    }

    @Test func connectingAgainReplacesThePassword() async throws {
        let cloud = model()
        try await cloud.connect(url: "https://dav.example.com", username: "me", password: "old")

        try await cloud.connect(url: "https://dav.example.com", username: "me", password: "new")

        #expect(credentials.load()?.password == "new")
        credentials.forget()
    }

    @Test func everyFailureHasItsOwnMessage() {
        let messages = [
            ConnectError.WrongCredentials, .Unreachable, .NotHttps, .NotWebDav, .ICloudDriveUnavailable, .WrongFolder,
        ].map(\.message)

        #expect(Set(messages).count == 6)
    }

    @Test func onlyWebDAVAndICloudDriveCanBeConnectedYet() {
        #expect(CloudProvider.allCases.filter(\.isAvailable) == [.webDAV, .iCloudDrive])
    }

    // MARK: iCloud Drive

    @Test func grantingICloudDriveShowsTheFolderAndCreatesIt() async throws {
        let root = try fakeICloudRoot()
        let cloud = model()

        try await cloud.connectICloudDrive(granted: root)

        #expect(cloud.connection == CloudConnection(provider: .iCloudDrive, detail: "ExcalidrawNative"))
        #expect(FileManager.default.fileExists(atPath: root.appending(path: "ExcalidrawNative").path))
        credentials.forget()
    }

    @Test func theGrantSurvivesARelaunch() async throws {
        let root = try fakeICloudRoot()
        try await model().connectICloudDrive(granted: root)

        let relaunched = model()

        #expect(relaunched.connection?.provider == .iCloudDrive)
        let bookmark = try #require(credentials.loadBookmark())
        #expect(ICloudGrant.resolve(bookmark)?.folder.standardizedFileURL.lastPathComponent == "com~apple~CloudDocs")
        credentials.forget()
    }

    @Test func anyOtherFolderIsRefusedAndNothingIsKept() async throws {
        let other = try fakeICloudRoot(named: "Documents")
        let cloud = model()

        await #expect(throws: ConnectError.WrongFolder) { try await cloud.connectICloudDrive(granted: other) }

        #expect(cloud.connection == nil)
        #expect(credentials.loadBookmark() == nil)
    }

    @Test func iCloudDriveBeingOffIsReportedBeforeThePickerIsShown() {
        #expect(throws: ConnectError.ICloudDriveUnavailable) {
            try model(iCloudDriveOn: false).prepareICloudDriveConnect()
        }
        #expect(throws: Never.self) { try model().prepareICloudDriveConnect() }
    }

    @Test func aLostGrantShowsAccessLostAndGrantingAgainResumes() async throws {
        let root = try fakeICloudRoot()
        try await model().connectICloudDrive(granted: root)
        try FileManager.default.removeItem(at: root.deletingLastPathComponent())
        let cloud = model()
        cloud.startSyncing(
            drawings: FileManager.default.temporaryDirectory.appending(path: UUID().uuidString),
            stateDirectory: FileManager.default.temporaryDirectory.appending(path: UUID().uuidString),
            onChanged: { _ in })

        #expect(cloud.status == .accessLost)
        #expect(cloud.hasProblem)
        #expect(cloud.statusText == "Access lost")
        #expect(cloud.connection?.provider == .iCloudDrive, "still Connected, to Reconnect")

        let again = try fakeICloudRoot()
        try await cloud.connectICloudDrive(granted: again)

        #expect(cloud.status != .accessLost)
        cloud.disconnect()
    }

    @Test func switchingFromICloudDriveToWebDAVDisconnectsTheGrant() async throws {
        let cloud = model()
        try await cloud.connectICloudDrive(granted: try fakeICloudRoot())

        try await cloud.connect(url: "https://dav.example.com", username: "me", password: "pw")

        #expect(cloud.connection?.provider == .webDAV)
        #expect(credentials.loadBookmark() == nil)
        credentials.forget()
    }

    // MARK: Problems and switching

    @Test func switchingProviderAsksForConfirmationAndThenDisconnects() async throws {
        let cloud = model()
        try await cloud.connect(url: "https://dav.example.com", username: "me", password: "pw")

        cloud.requestSwitch(to: .iCloudDrive)

        #expect(cloud.pendingSwitch == .iCloudDrive)
        #expect(cloud.connection != nil, "nothing is disconnected before the user confirms")
        #expect(cloud.confirmSwitch() == .iCloudDrive)
        #expect(cloud.connection == nil)
        #expect(credentials.load() == nil)
        #expect(cloud.pendingSwitch == nil)
    }

    @Test func cancellingASwitchKeepsTheConnection() async throws {
        let cloud = model()
        try await cloud.connect(url: "https://dav.example.com", username: "me", password: "pw")

        cloud.requestSwitch(to: .oneDrive)
        cloud.cancelSwitch()

        #expect(cloud.pendingSwitch == nil)
        #expect(cloud.connection != nil)
        credentials.forget()
    }

    @Test func pickingTheConnectedProviderAsksNothing() async throws {
        let cloud = model()
        try await cloud.connect(url: "https://dav.example.com", username: "me", password: "pw")

        cloud.requestSwitch(to: .webDAV)

        #expect(cloud.pendingSwitch == nil)
        credentials.forget()
    }

    @Test func reconnectingWithRefusedCredentialsKeepsTheOldOnes() async throws {
        try await model().connect(url: "https://dav.example.com", username: "me", password: "old")
        let cloud = model(checking: { _, _, _ in throw ConnectError.WrongCredentials })

        await #expect(throws: ConnectError.self) { try await cloud.reconnect(password: "wrong") }

        #expect(credentials.load()?.password == "old")
        credentials.forget()
    }

    @Test func reconnectingKeepsTheNewPassword() async throws {
        let cloud = model()
        try await cloud.connect(url: "https://dav.example.com", username: "me", password: "old")

        try await cloud.reconnect(password: "new")

        #expect(credentials.load()?.password == "new")
        #expect(cloud.connection?.detail == "https://dav.example.com")
        credentials.forget()
    }

    @Test func theStatusTextNamesTheProblem() async throws {
        let cloud = model()
        try await cloud.connect(url: "https://dav.example.com", username: "me", password: "pw")

        #expect(cloud.statusText == "Not synced yet")
        #expect(!cloud.hasProblem)
        credentials.forget()
    }
}
