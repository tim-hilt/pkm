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
///
/// A Drawing keeps its Item ID in its file (ADR-0019), so it survives renames,
/// and `list` gives every Drawing one.
struct DrawingStore: Sendable {
    static let fileExtension = "excalidraw"

    let directory: URL
    /// Holds the caches of the Drawings in `directory` and nothing else:
    /// whatever in it belongs to no Drawing there is removed.
    let cacheDirectory: URL

    /// The Item IDs read from the files, so that listing does not read every
    /// Drawing each time. Shared by the copies of the store.
    private let readItemIDs = ReadItemIDs()

    init(directory: URL, cacheDirectory: URL) {
        self.directory = directory
        self.cacheDirectory = cacheDirectory
    }

    /// A Drawing as listed in the Gallery.
    struct Entry: Equatable, Identifiable, Sendable {
        /// Nil for a file that is not a Drawing, which cannot hold one.
        let itemID: String?
        let name: String
        let modified: Date
        /// The Item ID, or the name of a file that has none.
        var id: String { itemID ?? name }
    }

    /// A name that is empty or would put the file somewhere else than in the
    /// store's folder.
    struct InvalidName: Error, Equatable { let name: String }

    /// Another Drawing already has the name.
    struct NameTaken: Error, Equatable { let name: String }

    /// The Drawings, last modified first. Gives each Drawing that has no Item
    /// ID one (a Drawing dropped into the folder, or saved by excalidraw.com),
    /// and each of several Drawings with the same one but the oldest a new
    /// one (a copy made in Finder); that rewrites their files.
    func list() throws -> [Entry] {
        let files = try FileManager.default.contentsOfDirectory(
            at: directory, includingPropertiesForKeys: [.contentModificationDateKey, .isDirectoryKey],
            options: .skipsHiddenFiles)
        var drawings = files.compactMap { file -> (name: String, modified: Date, itemID: String?)? in
            guard file.pathExtension == Self.fileExtension else { return nil }
            let values = try? file.resourceValues(forKeys: [.contentModificationDateKey, .isDirectoryKey])
            guard values?.isDirectory != true else { return nil }
            let name = file.deletingPathExtension().lastPathComponent
            let modified = values?.contentModificationDate ?? .distantPast
            return (name, modified, nil)
        }
        .sorted { ($0.modified, $0.name) < ($1.modified, $1.name) }

        for index in drawings.indices {
            drawings[index].itemID = readItemIDs.itemID(named: drawings[index].name, modified: drawings[index].modified)
            {
                (try? Data(contentsOf: directory.appending(path: "\(drawings[index].name).\(Self.fileExtension)")))
                    .flatMap { drawingItemId(bytes: $0) }
            }
        }
        let claimed = claimItemIds(oldestFirst: drawings.map(\.itemID))
        for (index, itemID) in claimed.enumerated() {
            guard let itemID else { continue }
            if let modified = giveItemID(itemID, toDrawingNamed: drawings[index].name) {
                drawings[index].itemID = itemID
                drawings[index].modified = modified
                readItemIDs.remember(itemID, ofDrawingNamed: drawings[index].name, modified: modified)
            } else {
                // Not a Drawing (or not writable): it has none; each listing tries again.
                drawings[index].itemID = nil
                readItemIDs.remember(nil, ofDrawingNamed: drawings[index].name, modified: drawings[index].modified)
            }
        }
        return
            drawings
            .map { Entry(itemID: $0.itemID, name: $0.name, modified: $0.modified) }
            .sorted { ($0.modified, $1.name) > ($1.modified, $0.name) }
    }

    /// The name of the Drawing with the Item ID `id` (an `Entry.id`), if there is one.
    func name(forItemID id: String) throws -> String? {
        try list().first { $0.id == id }?.name
    }

    /// Writes `itemID` into the Drawing's file and returns its new modification
    /// date, or nil if that cannot be done.
    private func giveItemID(_ itemID: String, toDrawingNamed name: String) -> Date? {
        guard let url = try? url(for: name), let bytes = try? Data(contentsOf: url),
            let stamped = try? drawingWithItemId(bytes: bytes, itemId: itemID),
            (try? stamped.write(to: url, options: .atomic)) != nil
        else { return nil }
        return try? url.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate
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
        try add(contents, named: "Untitled \(Self.format(date, in: timeZone))")
    }

    /// Makes a new Drawing holding `contents` named `name`, or "<name> 2",
    /// "<name> 3", … if that is taken, and returns the name. Never replaces
    /// another Drawing. It gets an Item ID if `contents` hold none.
    func add(_ contents: Data, named name: String) throws -> String {
        // A file that is no Drawing stays as it is.
        var contents = contents
        if drawingItemId(bytes: contents) == nil { contents = Self.withNewItemID(contents) }
        let name = try distinctName(from: name.trimmingCharacters(in: .whitespacesAndNewlines))
        // Left by a Drawing of that name deleted in Files or Finder.
        try? FileManager.default.removeItem(at: cacheDirectory(for: name))
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

    /// Which file a Drawing was, by the identity and modification date it had
    /// when the app last read or wrote it. Writing a file atomically (as the
    /// store, Files and sync clients do) makes it another file, so a stamp
    /// that no longer matches means the Drawing was changed by someone else.
    struct FileStamp: Equatable {
        fileprivate let identity: any NSObjectProtocol
        let modified: Date

        static func == (a: Self, b: Self) -> Bool {
            a.identity.isEqual(b.identity) && a.modified == b.modified
        }
    }

    /// The stamp of the Drawing's file now, or nil if there is no such file.
    func stamp(of name: String) -> FileStamp? {
        guard let url = try? url(for: name) else { return nil }
        return Self.stamp(of: url)
    }

    /// The name of the Drawing whose file is the one `stamp` stamped, if it
    /// now has another name (renamed in Files or Finder); nil if there is
    /// none, which is also what a deleted Drawing looks like.
    func name(matching stamp: FileStamp) -> String? {
        let files =
            (try? FileManager.default.contentsOfDirectory(
                at: directory, includingPropertiesForKeys: nil, options: .skipsHiddenFiles)) ?? []
        return files.first { $0.pathExtension == Self.fileExtension && Self.stamp(of: $0) == stamp }
            .map { $0.deletingPathExtension().lastPathComponent }
    }

    private static func stamp(of url: URL) -> FileStamp? {
        guard let values = try? url.resourceValues(forKeys: [.fileResourceIdentifierKey, .contentModificationDateKey]),
            let identity = values.fileResourceIdentifier, let modified = values.contentModificationDate
        else { return nil }
        return FileStamp(identity: identity, modified: modified)
    }

    /// Moves the caches of a Drawing that was renamed outside the app along
    /// with it. (`rename` does this for renames by the store.)
    func moveCaches(of name: String, to newName: String) {
        moveCache(of: name, to: newName)
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
    /// 2", …) and returns it. The copy is the newest Drawing and has an Item
    /// ID of its own.
    func duplicate(_ name: String) throws -> String {
        let contents = Self.withNewItemID(try open(name))
        return try add(contents, named: "\(name) copy")
    }

    private static func withNewItemID(_ contents: Data) -> Data {
        (try? drawingWithItemId(bytes: contents, itemId: newItemId())) ?? contents
    }

    /// `base`, or the first of "<base> 2", "<base> 3", … no Drawing has.
    private func distinctName(from base: String) throws -> String {
        var name = base
        var number = 2
        while FileManager.default.fileExists(atPath: try url(for: name).path) {
            name = "\(base) \(number)"
            number += 1
        }
        return name
    }

    /// Removes the Drawing and its caches. There is no way back.
    func delete(_ name: String) throws {
        try FileManager.default.removeItem(at: url(for: name))
        try? FileManager.default.removeItem(at: cacheDirectory(for: name))
    }

    /// Removes the caches of Drawings that are gone without the store's
    /// knowing (deleted or renamed in Files or Finder). Only frees disk space:
    /// a Drawing made under such a name later never picks up its caches anyway.
    func removeOrphanedCaches() {
        let folders = (try? FileManager.default.contentsOfDirectory(atPath: cacheDirectory.path)) ?? []
        for name in folders {
            let drawing = try? url(for: name)
            if drawing.map({ FileManager.default.fileExists(atPath: $0.path) }) != true {
                try? FileManager.default.removeItem(at: cacheDirectory(for: name))
            }
        }
    }

    /// Where the caches of the Drawing go. The folder may not exist.
    func cacheDirectory(for name: String) -> URL {
        cacheDirectory.appending(path: name, directoryHint: .isDirectory)
    }

    // MARK: Thumbnails

    /// One file per appearance: each is made once and stays until the Drawing changes.
    private static func thumbnailFile(dark: Bool) -> String { dark ? "thumbnail-dark.png" : "thumbnail.png" }

    /// The cached thumbnail (PNG) of the Drawing as it is listed in `entry`, in
    /// the light or dark `appearance` (ADR-0010), or nil if there is none or
    /// the Drawing changed since it was made.
    ///
    /// A thumbnail file is stamped with the modification date of the Drawing
    /// it shows, and is current exactly when that is still the Drawing's date:
    /// whatever changes the file, here or in Files, makes it stale.
    func thumbnail(for entry: Entry, dark: Bool = false) -> Data? {
        let file = cacheDirectory(for: entry.name).appending(
            path: Self.thumbnailFile(dark: dark), directoryHint: .notDirectory)
        guard let stamp = try? file.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate,
            abs(stamp.timeIntervalSince(entry.modified)) < Self.stampTolerance
        else { return nil }
        return try? Data(contentsOf: file)
    }

    /// Caches `png` as the thumbnail of the Drawing as it is listed in `entry`
    /// for the light or dark appearance; it is stale once the Drawing changes. Not a change of the Drawing.
    func saveThumbnail(_ png: Data, for entry: Entry, dark: Bool = false) throws {
        let folder = cacheDirectory(for: entry.name)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let file = folder.appending(path: Self.thumbnailFile(dark: dark), directoryHint: .notDirectory)
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
    /// Caches already under `newName` (of a Drawing deleted in Files or
    /// Finder) are dropped, also when the Drawing has none to move.
    private func moveCache(of name: String, to newName: String) {
        let source = cacheDirectory(for: name)
        let target = cacheDirectory(for: newName)
        if !Self.isSameFile(source, target) { try? FileManager.default.removeItem(at: target) }
        guard FileManager.default.fileExists(atPath: source.path) else { return }
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

    /// The Drawing's `.excalidraw` file itself, to hand to another app (share
    /// sheet). Throws if there is no such Drawing.
    func fileURL(for name: String) throws -> URL {
        let url = try url(for: name)
        guard FileManager.default.fileExists(atPath: url.path) else { throw CocoaError(.fileNoSuchFile) }
        return url
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

/// The Item IDs found in the Drawings' files, by name, valid while the file's
/// modification date is the one it was read at.
private final class ReadItemIDs: @unchecked Sendable {
    private let lock = NSLock()
    private var known: [String: (modified: Date, itemID: String?)] = [:]

    /// The remembered Item ID of the Drawing, or what `read` finds (and remembers).
    func itemID(named name: String, modified: Date, read: () -> String?) -> String? {
        lock.lock()
        let remembered = known[name]
        lock.unlock()
        if let remembered, remembered.modified == modified { return remembered.itemID }
        let itemID = read()
        remember(itemID, ofDrawingNamed: name, modified: modified)
        return itemID
    }

    func remember(_ itemID: String?, ofDrawingNamed name: String, modified: Date) {
        lock.lock()
        known[name] = (modified, itemID)
        lock.unlock()
    }
}
