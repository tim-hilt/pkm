import Foundation

/// The Drawings on the device: `<name>.excalidraw` files in one folder (the
/// app's Documents folder, so that Files and Finder show them; ADR-0003). The
/// file name is the Drawing's name. All file I/O lives here; the core only
/// sees bytes.
struct DrawingStore: Sendable {
    static let fileExtension = "excalidraw"

    let directory: URL

    /// A Drawing as listed in the Gallery.
    struct Entry: Equatable, Identifiable, Sendable {
        let name: String
        let modified: Date
        var id: String { name }
    }

    /// A name that is empty or would put the file somewhere else than in the
    /// store's folder.
    struct InvalidName: Error, Equatable { let name: String }

    /// The Drawings, last modified first.
    func list() throws -> [Entry] {
        let files = try FileManager.default.contentsOfDirectory(
            at: directory, includingPropertiesForKeys: [.contentModificationDateKey, .isDirectoryKey],
            options: .skipsHiddenFiles)
        return files.compactMap { file -> Entry? in
            guard file.pathExtension == Self.fileExtension else { return nil }
            let values = try? file.resourceValues(forKeys: [.contentModificationDateKey, .isDirectoryKey])
            guard values?.isDirectory != true else { return nil }
            return Entry(
                name: file.deletingPathExtension().lastPathComponent,
                modified: values?.contentModificationDate ?? .distantPast)
        }
        .sorted { ($0.modified, $1.name) > ($1.modified, $0.name) }
    }

    /// Makes a new Drawing named "Untitled <date time>" (made distinct if that
    /// name is taken) holding `contents`, and returns its name.
    func create(at date: Date = .now, in timeZone: TimeZone = .current, contents: Data) throws -> String {
        let base = "Untitled \(Self.format(date, in: timeZone))"
        var name = base
        var number = 2
        while FileManager.default.fileExists(atPath: try url(for: name).path) {
            name = "\(base) \(number)"
            number += 1
        }
        try save(contents, as: name)
        return name
    }

    /// The bytes of the Drawing.
    func open(_ name: String) throws -> Data {
        try Data(contentsOf: url(for: name))
    }

    /// Writes the Drawing atomically: the file is whole with either the old
    /// or the new contents, whatever happens part-way.
    func save(_ contents: Data, as name: String) throws {
        try contents.write(to: url(for: name), options: .atomic)
    }

    private func url(for name: String) throws -> URL {
        guard !name.isEmpty, !name.contains("/"), name != ".", name != ".." else {
            throw InvalidName(name: name)
        }
        return directory.appending(path: "\(name).\(Self.fileExtension)", directoryHint: .notDirectory)
    }

    /// `2026-11-03 19.27.31`: no colons, which Finder shows as slashes.
    private static func format(_ date: Date, in timeZone: TimeZone) -> String {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = timeZone
        formatter.dateFormat = "yyyy-MM-dd HH.mm.ss"
        return formatter.string(from: date)
    }
}
