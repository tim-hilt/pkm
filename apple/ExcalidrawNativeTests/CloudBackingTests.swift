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

    private func model(checking check: @escaping @Sendable (String, String, String) throws -> Void = { _, _, _ in })
        -> CloudBackingModel
    {
        CloudBackingModel(credentials: credentials, checkWebDAV: check)
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
        let messages = [ConnectError.WrongCredentials, .Unreachable, .NotHttps, .NotWebDav].map(\.message)

        #expect(Set(messages).count == 4)
    }

    @Test func onlyWebDAVCanBeConnectedYet() {
        #expect(CloudProvider.allCases.filter(\.isAvailable) == [.webDAV])
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
