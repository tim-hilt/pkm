import CoreTransferable
import SwiftUI
import UniformTypeIdentifiers

/// The Notes Context: the flat list of Notes beside the open Note's editor where there is room
/// (Mac, iPad); on an iPhone the list with each Note pushed over it.
struct NotesView: View {
    @Environment(NotesModel.self) private var model
    @Environment(\.horizontalSizeClass) private var sizeClass

    var body: some View {
        if NotesLayout.pushes(sizeClass) {
            NotesBrowser(pushes: true)
                .addNoteButton()
                .navigationDestination(for: String.self) { NoteEditor(id: $0).id($0) }
        } else {
            HStack(spacing: 0) {
                NotesBrowser(pushes: false).frame(width: 300)
                Divider()
                if let id = model.openID {
                    NoteEditor(id: id).id(id)
                } else {
                    ContentUnavailableView("No Note Selected", systemImage: "note.text")
                }
            }
            .addNoteButton()
        }
    }
}

enum NotesLayout {
    /// Whether the editor is pushed over the list rather than shown beside it. Then the list has
    /// no selection: a selectable List takes a `NavigationLink(value:)`'s tap as a selection of
    /// the value and pushes nothing.
    static func pushes(_ sizeClass: UserInterfaceSizeClass?) -> Bool { sizeClass == .compact }
}

/// The Notes newest changed first; while searching, the results instead. Copy and Delete in a
/// row's context menu, Delete by swipe, and on the Mac with the Delete key.
private struct NotesBrowser: View {
    @Environment(NotesModel.self) private var model
    /// Whether a row pushes its editor, rather than selecting.
    let pushes: Bool
    @State private var deletingNote: String?

    private var isSearching: Bool { !model.searchText.isEmpty }

    var body: some View {
        list
            .confirmationDialog(
                "Delete this Note?",
                isPresented: Binding(get: { deletingNote != nil }, set: { if !$0 { deletingNote = nil } }),
                presenting: deletingNote
            ) { id in
                Button("Delete", role: .destructive) { model.delete(id: id) }
            }
            .accessibilityIdentifier("notes-browser")
    }

    private var list: some View {
        List(selection: pushes ? nil : Bindable(model).openID) {
            if isSearching {
                ForEach(model.results, id: \.id) { result in
                    row(id: result.id) { ResultRow(result: result) }
                }
            } else {
                ForEach(model.notes, id: \.id) { note in
                    row(id: note.id) { NoteRow(row: note) }
                        .swipeActions { Button("Delete", role: .destructive) { delete(note.id) } }
                        .contextMenu {
                            Button("Copy Note", systemImage: "doc.on.doc") { model.copy(id: note.id) }
                            Button("Delete", systemImage: "trash", role: .destructive) { delete(note.id) }
                        }
                }
            }
        }
        .searchable(text: Bindable(model).searchText)
        #if os(iOS)
            .searchToolbarBehavior(.minimize)
            .toolbar {
                // At the top, so that the bottom stays free for the "+", as in Books.
                DefaultToolbarItem(kind: .search, placement: .topBarTrailing)
            }
        #endif
        .overlay {
            if isSearching {
                if model.results.isEmpty { ContentUnavailableView.search(text: model.searchText) }
            } else if model.notes.isEmpty {
                ContentUnavailableView("No Notes", systemImage: "note.text", description: Text("Tap + to write one."))
            }
        }
        #if os(macOS)
            .onDeleteCommand {
                if let id = model.openID { delete(id) }
            }
        #endif
    }

    /// A row of the list: a link where the editor is pushed, else selectable.
    @ViewBuilder
    private func row(id: String, @ViewBuilder content: () -> some View) -> some View {
        if pushes {
            NavigationLink(value: id, label: content).tag(id)
        } else {
            content().tag(id)
        }
    }

    /// A Note with text is only Deleted after asking.
    private func delete(_ id: String) {
        if model.confirmsDelete(id: id) { deletingNote = id } else { model.delete(id: id) }
    }
}

/// The "+" for a new Note.
private struct AddNoteButton: ViewModifier {
    @Environment(NotesModel.self) private var model

    func body(content: Content) -> some View {
        content.addButton("New Note", identifier: "add-note") { model.add() }
    }
}

extension View {
    fileprivate func addNoteButton() -> some View { modifier(AddNoteButton()) }
}

/// A Search result: the Title and the snippet with the match emphasized.
private struct ResultRow: View {
    let result: SearchResult

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            if result.title.isEmpty {
                Text(snippet).lineLimit(2)
            } else {
                Text(AttributedString(title: result.title)).lineLimit(1)
                Text(snippet).lineLimit(2).font(.subheadline).foregroundStyle(.secondary)
            }
        }
    }

    private var snippet: AttributedString {
        AttributedString(
            snippet: result.snippet, matchedStart: result.matchedStart, matchedEnd: result.matchedEnd)
    }
}

/// A Note in the list: its Title formatted, or the Body's first line, or "New Note", and its Tags.
private struct NoteRow: View {
    let row: ListedNote

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            if !row.title.isEmpty {
                Text(AttributedString(title: row.title)).lineLimit(1)
                if !row.preview.isEmpty {
                    Text(AttributedString(title: row.preview)).lineLimit(1).font(.subheadline)
                        .foregroundStyle(.secondary)
                }
            } else if !row.preview.isEmpty {
                Text(AttributedString(title: row.preview)).lineLimit(1)
            } else {
                Text("New Note").foregroundStyle(.secondary)
            }
            if !row.tags.isEmpty {
                HStack(spacing: 4) { ForEach(row.tags, id: \.self) { TagPill(tag: $0) } }
                    .font(.caption).lineLimit(1)
            }
        }
    }
}

/// The Title and Body of one Note, saved as typed.
private struct NoteEditor: View {
    @Environment(NotesModel.self) private var model
    @Environment(TasksModel.self) private var tasks
    let id: String
    @State private var title = ""
    @State private var tags: [String] = []
    @State private var isPickingTags = false
    @State private var text = ""
    /// The Note is read; what is typed before is no edit.
    @State private var isLoaded = false

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                TitleField(text: $title, autofocus: model.newID == id)
                TagsRow(tags: tags) { isPickingTags = true }
                MarkdownEditor(text: $text, placeholder: "Note", images: model.images(for: id))
                    .accessibilityIdentifier("note-body")
            }
            .padding()
        }
        .onAppear {
            guard !isLoaded, let note = model.note(id: id) else { return }
            (title, text, tags) = (note.title, note.body, note.tags)
            isLoaded = true
        }
        .onChange(of: title) { save() }
        .onChange(of: text) { save() }
        // Sync may have merged another Device's edits in: shown, the next keystroke keeps them.
        .onChange(of: model.syncs) {
            guard isLoaded, let note = model.note(id: id) else { return }
            if note.title != title { title = note.title }
            if note.body != text { text = note.body }
            tags = note.tags
        }
        .sheet(isPresented: $isPickingTags) {
            TagPicker(selected: tags) {
                model.setTags($0, of: id)
                tags = model.note(id: id)?.tags ?? tags
                tasks.refreshTags()
                isPickingTags = false
            }
        }
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Menu("More", systemImage: "ellipsis.circle") {
                    Button("Copy Note", systemImage: "doc.on.doc") { model.copy(id: id) }
                }
            }
        }
        #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
        #endif
    }

    private func save() {
        guard isLoaded else { return }
        model.save(id: id, title: title, body: text)
    }
}
