import Foundation

/// The Drawings on the device: `<name>.excalidraw` files in one folder (the
/// app's Documents folder, so that Files and Finder show them; ADR-0003). The
/// file name is the Drawing's name. All file I/O lives here; the core only
/// sees bytes.
///
/// Whatever the app derives from a Drawing (viewport, thumbnail) is cached
/// outside that folder, in one subfolder per Drawing (`cacheDirectory(for:)`),
/// so that it is neither shown in Files nor synced. A cache follows its Drawing
/// when that is renamed and goes when it is deleted; it is never copied.
struct DrawingStore: Sendable {
    static let fileExtension = "excalidraw"

    let directory: URL
    let cacheDirectory: URL

    init(
        directory: URL, cacheDirectory: URL = .cachesDirectory.appending(path: "Drawings", directoryHint: .isDirectory)
    ) {
        self.directory = directory
        self.cacheDirectory = cacheDirectory
    }

    /// A Drawing as listed in the Gallery.
    struct Entry: Equatable, Identifiable, Sendable {
        let name: String
        let modified: Date
        var id: String { name }
    }

    /// A name that is empty or would put the file somewhere else than in the
    /// store's folder.
    struct InvalidName: Error, Equatable { let name: String }

    /// Another Drawing already has the name.
    struct NameTaken: Error, Equatable { let name: String }

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

    /// Reports (one `()` per burst) that Drawings were added, removed or
    /// renamed in the folder, by this store or by anyone else, as Files and
    /// Finder do. Watches while the stream is being iterated.
    ///
    /// A file written in place, without its name changing, is not reported;
    /// whoever shows the Drawings lists them again when it comes to the front.
    func changes() -> AsyncStream<Void> {
        AsyncStream(bufferingPolicy: .bufferingNewest(1)) { continuation in
            let descriptor = Darwin.open(directory.path, O_EVTONLY)
            guard descriptor >= 0 else { return continuation.finish() }
            let source = DispatchSource.makeFileSystemObjectSource(
                fileDescriptor: descriptor, eventMask: [.write, .rename, .delete, .link],
                queue: .global(qos: .utility))
            source.setEventHandler { continuation.yield() }
            source.setCancelHandler { close(descriptor) }
            continuation.onTermination = { _ in source.cancel() }
            source.resume()
        }
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

    /// Gives the Drawing another name and returns it (the given one without
    /// surrounding whitespace). Throws `NameTaken` rather than replace another
    /// Drawing; the same name, or only another case of it, is fine.
    func rename(_ name: String, to newName: String) throws -> String {
        let newName = newName.trimmingCharacters(in: .whitespacesAndNewlines)
        let source = try url(for: name)
        let target = try url(for: newName)
        guard FileManager.default.fileExists(atPath: source.path) else {
            throw CocoaError(.fileNoSuchFile)
        }
        guard newName != name else { return name }
        if FileManager.default.fileExists(atPath: target.path), !Self.isSameFile(source, target) {
            throw NameTaken(name: newName)
        }
        try Self.move(source, to: target)
        moveCache(of: name, to: newName)
        return newName
    }

    /// Copies the Drawing under a distinct name ("<name> copy", "<name> copy
    /// 2", …) and returns it. The copy is the newest Drawing.
    func duplicate(_ name: String) throws -> String {
        let contents = try open(name)
        let base = "\(name) copy"
        var copy = base
        var number = 2
        while FileManager.default.fileExists(atPath: try url(for: copy).path) {
            copy = "\(base) \(number)"
            number += 1
        }
        try save(contents, as: copy)
        return copy
    }

    /// Removes the Drawing and its caches. There is no way back.
    func delete(_ name: String) throws {
        try FileManager.default.removeItem(at: url(for: name))
        try? FileManager.default.removeItem(at: cacheDirectory(for: name))
    }

    /// Where the caches of the Drawing go. The folder may not exist.
    func cacheDirectory(for name: String) -> URL {
        cacheDirectory.appending(path: name, directoryHint: .isDirectory)
    }

    // MARK: Thumbnails

    private static let thumbnailFile = "thumbnail.png"

    /// The cached thumbnail (PNG) of the Drawing as it is listed in `entry`, or
    /// nil if there is none or the Drawing changed since it was made.
    ///
    /// A thumbnail file is stamped with the modification date of the Drawing
    /// it shows, and is current exactly when that is still the Drawing's date:
    /// whatever changes the file, here or in Files, makes it stale.
    func thumbnail(for entry: Entry) -> Data? {
        let file = cacheDirectory(for: entry.name).appending(path: Self.thumbnailFile, directoryHint: .notDirectory)
        guard let stamp = try? file.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate,
            abs(stamp.timeIntervalSince(entry.modified)) < Self.stampTolerance
        else { return nil }
        return try? Data(contentsOf: file)
    }

    /// Caches `png` as the thumbnail of the Drawing as it is listed in `entry`;
    /// it is stale once the Drawing changes. Not a change of the Drawing.
    func saveThumbnail(_ png: Data, for entry: Entry) throws {
        let folder = cacheDirectory(for: entry.name)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let file = folder.appending(path: Self.thumbnailFile, directoryHint: .notDirectory)
        try png.write(to: file, options: .atomic)
        // Between the two steps the file is merely stale, never wrong.
        try FileManager.default.setAttributes([.modificationDate: entry.modified], ofItemAtPath: file.path)
    }

    // MARK: Viewport

    private static let viewportFile = "viewport.json"

    /// The viewport (scroll and zoom) the Drawing was left at, or nil if none
    /// was cached or it cannot be read. It is the app's own state and not part
    /// of the Drawing, so it lives in the cache and never in the file.
    func viewport(for name: String) -> ViewportState? {
        let file = cacheDirectory(for: name).appending(path: Self.viewportFile, directoryHint: .notDirectory)
        guard (try? url(for: name)) != nil, let data = try? Data(contentsOf: file),
            let saved = try? JSONDecoder().decode(CachedViewport.self, from: data)
        else { return nil }
        return ViewportState(originX: saved.originX, originY: saved.originY, zoom: saved.zoom)
    }

    /// Caches `viewport` as the one to open the Drawing at. Not a change of
    /// the Drawing.
    func saveViewport(_ viewport: ViewportState, for name: String) throws {
        _ = try url(for: name)
        let saved = CachedViewport(originX: viewport.originX, originY: viewport.originY, zoom: viewport.zoom)
        try writeCache(JSONEncoder().encode(saved), named: Self.viewportFile, for: name)
    }

    /// The cache file's own format, so that it does not change with the
    /// generated type.
    private struct CachedViewport: Codable {
        let originX: Double
        let originY: Double
        let zoom: Double
    }

    private func writeCache(_ data: Data, named file: String, for name: String) throws {
        let folder = cacheDirectory(for: name)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        try data.write(to: folder.appending(path: file, directoryHint: .notDirectory), options: .atomic)
    }

    /// File dates are stored finer than this; it only absorbs rounding.
    private static let stampTolerance: TimeInterval = 0.001

    /// Caches can be made again, so a failure here costs nothing but time.
    private func moveCache(of name: String, to newName: String) {
        let source = cacheDirectory(for: name)
        let target = cacheDirectory(for: newName)
        guard FileManager.default.fileExists(atPath: source.path) else { return }
        if !Self.isSameFile(source, target) { try? FileManager.default.removeItem(at: target) }
        do { try Self.move(source, to: target) } catch { try? FileManager.default.removeItem(at: source) }
    }

    /// Moves, also when the two names differ only in case on a volume that
    /// ignores case, where they are the same item.
    private static func move(_ source: URL, to target: URL) throws {
        guard isSameFile(source, target) else {
            return try FileManager.default.moveItem(at: source, to: target)
        }
        let aside = source.deletingLastPathComponent().appending(path: UUID().uuidString)
        try FileManager.default.moveItem(at: source, to: aside)
        try FileManager.default.moveItem(at: aside, to: target)
    }

    private static func isSameFile(_ a: URL, _ b: URL) -> Bool {
        let keys: Set<URLResourceKey> = [.fileResourceIdentifierKey]
        guard let idA = try? a.resourceValues(forKeys: keys).fileResourceIdentifier,
            let idB = try? b.resourceValues(forKeys: keys).fileResourceIdentifier
        else { return false }
        return idA.isEqual(idB)
    }

    private func url(for name: String) throws -> URL {
        guard !name.isEmpty, !name.contains("/"), !name.hasPrefix(".") else {
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
