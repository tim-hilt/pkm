import Foundation
import Testing

/// Reference images (PNG files in a directory) that renders are checked against.
///
/// Normal runs only compare. Recording is the one explicit way to change
/// references; see docs/renderer-reference-images.md.
struct ReferenceStore {
    enum Outcome: Equatable {
        case matched
        /// Recording mode wrote a new or changed reference; review it before committing.
        case recorded
        /// The message says what is wrong and what to do about it.
        case failed(String)
    }

    /// Holds `<name>.png` per reference.
    let directory: URL
    /// Receives `<name>-actual.png` and `<name>-diff.png` for failed checks, so
    /// a regression can be looked at; removed again once the check passes. The
    /// project's store uses the temporary directory because the sandboxed test
    /// host may write nowhere else.
    let failuresDirectory: URL
    let recording: Bool

    /// The references committed with the tests. Recording is switched on by the
    /// `RECORD_REFERENCES` environment variable (`make record-references`).
    static let project: ReferenceStore = {
        let tests = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
        return ReferenceStore(
            directory: tests.appending(path: "References"),
            failuresDirectory: FileManager.default.temporaryDirectory.appending(path: "WorkingTitleReferenceFailures"),
            recording: ProcessInfo.processInfo.environment["RECORD_REFERENCES"] != nil)
    }()

    /// Compares `image` with the reference called `name`. When recording, a
    /// missing or no longer matching reference is replaced by `image` instead of failing.
    func check(_ image: RenderedImage, named name: String, tolerance: ImageTolerance = .standard) -> Outcome {
        let file = directory.appending(path: "\(name).png")
        let actualFile = failuresDirectory.appending(path: "\(name)-actual.png")
        let diffFile = failuresDirectory.appending(path: "\(name)-diff.png")

        let reference = (try? Data(contentsOf: file)).flatMap { RenderedImage(pngData: $0) }
        let mismatch = reference.flatMap { ImageComparison.mismatch(between: image, and: $0, tolerance: tolerance) }

        if reference != nil, mismatch == nil {
            for stale in [actualFile, diffFile] { try? FileManager.default.removeItem(at: stale) }
            return .matched
        }
        if recording {
            return write(image.pngData(), to: file).map(Outcome.failed) ?? .recorded
        }
        guard reference != nil, let mismatch else {
            return .failed("no reference image \(file.path); record it with `make record-references`")
        }

        let writeErrors = [
            write(image.pngData(), to: actualFile), mismatch.diff.flatMap { write($0.pngData(), to: diffFile) },
        ].compactMap { $0 }
        return .failed(
            "\(name): \(mismatch.summary). Compare \(actualFile.path) with \(file.path), "
                + "differences in \(diffFile.path). If the change is intended, run `make record-references`."
                + writeErrors.map { " (\($0))" }.joined())
    }

    /// Returns an error message, or nil on success.
    private func write(_ data: Data, to file: URL) -> String? {
        do {
            try FileManager.default.createDirectory(
                at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
            try data.write(to: file)
            return nil
        } catch {
            return "cannot write \(file.path): \(error.localizedDescription)"
        }
    }
}

/// Fails the current test unless `image` matches the reference image `name`.
func expectMatchesReference(
    _ image: RenderedImage, named name: String, tolerance: ImageTolerance = .standard,
    sourceLocation: SourceLocation = #_sourceLocation
) {
    if case .failed(let message) = ReferenceStore.project.check(image, named: name, tolerance: tolerance) {
        Issue.record(Comment(rawValue: message), sourceLocation: sourceLocation)
    }
}
