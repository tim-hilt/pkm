import Foundation
import Testing

@testable import pkm

/// Which credentials a launch gets. Only a real launch may touch the Keychain:
/// elsewhere it prompts for access after every rebuild of a Debug build.
@Suite struct CloudCredentialsTests {
    @Test func aRealLaunchKeepsSecretsInTheKeychain() {
        let credentials = CloudCredentials.forLaunch(arguments: ["pkm"], environment: [:])

        #expect((credentials.secrets as? KeychainSecrets)?.service == "com.timhilt.pkm.cloud")
    }

    @Test func uiTestsAndScreenshotsKeepSecretsInMemory() {
        let credentials = CloudCredentials.forLaunch(arguments: ["pkm", "-UITestingOpenSettings"], environment: [:])

        #expect(credentials.secrets is MemorySecrets)
        #expect(credentials.load() == nil)
    }

    @Test func hostingUnitTestsKeepsSecretsInMemory() {
        let credentials = CloudCredentials.forLaunch(
            arguments: ["pkm"], environment: ["XCTestConfigurationFilePath": "/tmp/x.xctestconfiguration"])

        #expect(credentials.secrets is MemorySecrets)
    }

    @Test func memorySecretsAreReplacedAndDeleted() {
        let secrets = MemorySecrets()

        #expect(secrets.write("a", as: "token"))
        #expect(secrets.write("b", as: "token"))
        #expect(secrets.read("token") == "b")
        secrets.delete("token")
        #expect(secrets.read("token") == nil)
    }
}
