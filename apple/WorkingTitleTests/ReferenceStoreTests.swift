import Foundation
import Testing

@testable import WorkingTitle

/// The store's contract with a developer: what happens on first run, on a
/// match, on a regression and when deliberately re-recording.
@Suite struct ReferenceStoreTests {
    private let root = FileManager.default.temporaryDirectory.appending(path: "ReferenceStoreTests-\(UUID())")
    private var references: URL { root.appending(path: "references") }
    private var failures: URL { root.appending(path: "failures") }

    private func store(recording: Bool = false) -> ReferenceStore {
        ReferenceStore(directory: references, failuresDirectory: failures, recording: recording)
    }

    private func image(grey: UInt8) -> RenderedImage {
        RenderedImage(width: 20, height: 20, bgra: [UInt8](repeating: grey, count: 20 * 20 * 4))
    }

    private func exists(_ url: URL) -> Bool { FileManager.default.fileExists(atPath: url.path) }

    @Test func missingReferenceFailsAndSaysHowToRecordIt() throws {
        let outcome = store().check(image(grey: 255), named: "new")
        guard case .failed(let message) = outcome else {
            Issue.record("expected a failure, got \(outcome)")
            return
        }
        #expect(message.contains("make record-references"))
        #expect(!exists(references.appending(path: "new.png")))
    }

    @Test func recordingWritesTheReferenceAndLaterRunsMatchIt() {
        #expect(store(recording: true).check(image(grey: 255), named: "new") == .recorded)
        #expect(exists(references.appending(path: "new.png")))
        #expect(store().check(image(grey: 255), named: "new") == .matched)
    }

    @Test func recordingLeavesAMatchingReferenceUntouched() throws {
        _ = store(recording: true).check(image(grey: 100), named: "stable")
        let file = references.appending(path: "stable.png")
        let before = try Data(contentsOf: file)
        // Within tolerance: would be churn in version control if rewritten.
        #expect(store(recording: true).check(image(grey: 110), named: "stable") == .matched)
        #expect(try Data(contentsOf: file) == before)
    }

    @Test func recordingReplacesAChangedReference() throws {
        _ = store(recording: true).check(image(grey: 0), named: "changed")
        #expect(store(recording: true).check(image(grey: 255), named: "changed") == .recorded)
        #expect(store().check(image(grey: 255), named: "changed") == .matched)
    }

    @Test func regressionFailsAndLeavesActualAndDiffImagesBehind() throws {
        _ = store(recording: true).check(image(grey: 0), named: "regress")
        let outcome = store().check(image(grey: 255), named: "regress")
        guard case .failed(let message) = outcome else {
            Issue.record("expected a failure, got \(outcome)")
            return
        }
        #expect(message.contains("regress-diff.png"))
        let actual = try #require(
            RenderedImage(pngData: Data(contentsOf: failures.appending(path: "regress-actual.png"))))
        #expect(actual == image(grey: 255))
        #expect(exists(failures.appending(path: "regress-diff.png")))
    }

    @Test func passingRunRemovesStaleFailureImages() {
        _ = store(recording: true).check(image(grey: 0), named: "healed")
        _ = store().check(image(grey: 255), named: "healed")
        #expect(store().check(image(grey: 0), named: "healed") == .matched)
        #expect(!exists(failures.appending(path: "healed-actual.png")))
        #expect(!exists(failures.appending(path: "healed-diff.png")))
    }
}
