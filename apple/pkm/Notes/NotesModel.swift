import Foundation
import Observation

#if canImport(AppKit)
    import AppKit
#elseif canImport(UIKit)
    import UIKit
#endif

/// The Notes Context's state: the flat list of Notes and the open Note, from the core's store. It only forwards to the store; every Notes rule lives in the core.
@MainActor @Observable
final class NotesModel {
    private let store: NoteStore
    @ObservationIgnored private let defaults: UserDefaults?
    private static let noteKey = "notesLastNote"

    /// The Notes and the Tags in use, read since the files last changed; read again on demand.
    @ObservationIgnored private var listed: [ListedNote]?
    @ObservationIgnored private var used: [String]?

    /// The Tag the user narrowed the list to, even if no Note carries it any more.
    private var chosenTag: String?

    /// The Tag the list is narrowed to; nil for "All Notes", which it drops back to once no Note
    /// carries its Tag.
    var tagFilter: String? {
        get { chosenTag.flatMap { tagsInUse.contains($0) ? $0 : nil } }
        set {
            chosenTag = newValue
            listed = nil
        }
    }

    /// The Tags some Note carries, alphabetically: what the list can be narrowed to.
    var tagsInUse: [String] {
        _ = changes
        if let used { return used }
        let tags = (try? store.tagsInUse()) ?? []
        used = tags
        return tags
    }

    /// Grows when the files change, so that views asking for `notes` read them again.
    private var changes = 0

    /// What Search looks for; empty when not searching.
    var searchText = "" {
        didSet { if searchText != oldValue { reloadResults() } }
    }

    /// The Notes that match `searchText`, Title matches first; empty when not searching.
    private(set) var results: [SearchResult] = []

    /// The open Note's ID. Leaving a Note (opening another, or none) discards it if it is empty.
    var openID: String? {
        didSet {
            guard oldValue != openID else { return }
            if let oldValue {
                try? store.releaseImages(id: oldValue, references: Array(imagesInPlay))
                try? store.discardIfEmpty(id: oldValue)
            }
            imagesInPlay = openID.flatMap { note(id: $0) }.map { Set(imageTargets(text: $0.body)) } ?? []
            if openID != newID { newID = nil }
            defaults?.set(openID, forKey: Self.noteKey)
            filesChanged()
        }
    }

    /// The Images the open Note started with or added, until it is left.
    @ObservationIgnored private var imagesInPlay: Set<String> = []

    /// The Note just added and not yet left: its editor takes the focus on the Title.
    private(set) var newID: String?

    /// How often Sync changed the files: when it grows, an open editor takes the Note as saved.
    private(set) var syncs = 0

    /// `defaults` holds the last Note, which opens again unless it is gone; `nil` remembers nothing.
    init(store: NoteStore, defaults: UserDefaults? = nil) {
        self.store = store
        self.defaults = defaults
        if let id = defaults?.string(forKey: Self.noteKey), notes.contains(where: { $0.id == id }) {
            openID = id
        }
    }

    // MARK: Navigation

    /// What is pushed over the list, for a narrow window: the open Note's ID.
    var pushedPath: [String] {
        get { openID.map { [$0] } ?? [] }
        set { openID = newValue.last }
    }

    // MARK: Notes

    /// Adds an empty Note and opens it, for its Title to be typed.
    func add() {
        guard let note = try? store.create(tags: tagFilter.map { [$0] } ?? []) else { return }
        newID = note.id
        openID = note.id
    }

    /// The Note `id` as last saved.
    func note(id: String) -> Note? { try? store.read(id: id) }

    /// Saves the Title and Body as typed.
    func save(id: String, title: String, body: String) {
        guard (try? store.edit(id: id, title: title, body: body)) != nil else { return }
        listed = nil
        used = nil
        changes += 1
        reloadResults()
    }

    /// Gives the Note exactly `tags`: those it lacks are added in order, the others removed. Tags
    /// the app didn't know become known to the core's Tags file.
    func setTags(_ tags: [String], of id: String) {
        guard let current = note(id: id)?.tags else { return }
        for tag in current where !tags.contains(tag) { _ = try? store.removeTag(id: id, tag: tag) }
        for tag in tags where !current.contains(tag) { _ = try? store.addTag(id: id, tag: tag) }
        filesChanged()
    }

    /// Takes `tag` off every Note, as part of Delete a Tag.
    func removeTagEverywhere(_ tag: String) {
        try? store.removeTagEverywhere(tag: tag)
        filesChanged()
    }

    /// The Note as text to copy: its Title, then its Body after a blank line; either may be missing.
    func copyText(id: String) -> String {
        guard let note = note(id: id) else { return "" }
        return [note.title, note.body].filter { !$0.isEmpty }.joined(separator: "\n\n")
    }

    /// Puts the Note's text on the clipboard.
    func copy(id: String) {
        let text = copyText(id: id)
        #if os(macOS)
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(text, forType: .string)
        #else
            UIPasteboard.general.string = text
        #endif
    }

    /// Whether Deleting the Note asks first: its Body has text.
    func confirmsDelete(id: String) -> Bool {
        !(note(id: id)?.body.isEmpty ?? true)
    }

    /// Deletes the Note; the open one closes.
    func delete(id: String) {
        if openID == id { openID = nil }
        try? store.delete(id: id)
        filesChanged()
    }

    // MARK: Images

    /// Keeps a picture beside the Note `id`, as PNG unless it is a PNG, JPEG, GIF or WebP; the
    /// reference for its Body. Nil if it is no image. Leaving the Note Deletes it if the Body
    /// doesn't show it.
    func addImage(_ picture: Data, to id: String) -> String? {
        let reference = StoredImage.bytes(of: picture).flatMap { try? store.addImage(id: id, bytes: $0) }
        if let reference, id == openID { imagesInPlay.insert(reference) }
        return reference
    }

    /// The bytes of the Image `reference` names beside the Note `id`.
    func image(_ reference: String, of id: String) -> Data? { try? store.readImage(id: id, reference: reference) }

    /// What the Body editor of the Note `id` keeps its Images with.
    func images(for id: String) -> ImageSource {
        ImageSource(add: { [self] in addImage($0, to: id) }, read: { [self] in image($0, of: id) })
    }

    /// The Notes, newest changed first: those carrying `tagFilter`, or all.
    var notes: [ListedNote] {
        _ = changes
        if let listed { return listed }
        let notes = (try? tagFilter.map { try store.notesTagged(tag: $0) } ?? store.notes()) ?? []
        listed = notes
        return notes
    }

    /// Sync wrote or deleted Note files: the list shows what is there now, and an open Note it
    /// Deleted closes.
    func syncDidChange() {
        if let openID, note(id: openID) == nil { self.openID = nil }
        syncs += 1
        filesChanged()
    }

    /// Notes were added or Deleted: the list reads them again.
    private func filesChanged() {
        listed = nil
        used = nil
        changes += 1
        reloadResults()
    }

    private func reloadResults() {
        results = searchText.isEmpty ? [] : (try? store.search(query: searchText)) ?? []
    }
}
