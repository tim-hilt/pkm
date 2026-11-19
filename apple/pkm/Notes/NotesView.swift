import SwiftUI

/// The Notes Context: the list of Notes, newest changed first, and the open Note's editor, beside
/// the list where there is room (Mac, iPad) and pushed over it on an iPhone. A Note is Deleted by
/// swipe, context menu or ⌫ on the Mac, asking first when its Body has text.
struct NotesView: View {
    @Environment(NotesModel.self) private var model
    @Environment(\.horizontalSizeClass) private var sizeClass
    @State private var deleting: ListedNote?

    var body: some View {
        Group {
            if NotesLayout.pushes(sizeClass) {
                list
            } else {
                HStack(spacing: 0) {
                    list.frame(width: 300)
                    Divider()
                    if let id = model.openID {
                        NoteEditor(id: id).id(id)
                    } else {
                        ContentUnavailableView("No Note Selected", systemImage: "note.text")
                    }
                }
            }
        }
        .navigationDestination(for: String.self) { NoteEditor(id: $0).id($0) }
        .addButton("New Note", identifier: "add-note") { model.add() }
        .confirmationDialog(
            "Delete this Note?", isPresented: Binding(get: { deleting != nil }, set: { if !$0 { deleting = nil } }),
            presenting: deleting
        ) { row in
            Button("Delete", role: .destructive) { model.delete(id: row.id) }
        }
    }

    @ViewBuilder private var list: some View {
        List(selection: NotesLayout.pushes(sizeClass) ? .constant(nil) : Bindable(model).openID) {
            ForEach(model.rows, id: \.id) { row in
                Group {
                    if NotesLayout.pushes(sizeClass) {
                        NavigationLink(value: row.id) { NoteRow(row: row) }
                    } else {
                        NoteRow(row: row)
                    }
                }
                .tag(row.id)
                .swipeActions { Button("Delete", role: .destructive) { ask(row) } }
                .contextMenu { Button("Delete", systemImage: "trash", role: .destructive) { ask(row) } }
            }
        }
        .overlay {
            if model.rows.isEmpty {
                ContentUnavailableView("No Notes", systemImage: "note.text", description: Text("Tap + to write one."))
            }
        }
        #if os(macOS)
            .onDeleteCommand {
                if let id = model.openID, let row = model.rows.first(where: { $0.id == id }) { ask(row) }
            }
        #endif
        .accessibilityIdentifier("notes-list")
    }

    /// Deletes the Note, or asks first if its Body has text.
    private func ask(_ row: ListedNote) {
        if model.confirmsDelete(id: row.id) { deleting = row } else { model.delete(id: row.id) }
    }
}

enum NotesLayout {
    /// Whether the editor is pushed over the list rather than shown beside it.
    static func pushes(_ sizeClass: UserInterfaceSizeClass?) -> Bool { sizeClass == .compact }
}

/// A Note in the list: its Title formatted, or the Body's first line, or "New Note".
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
        }
    }
}

/// The Title and Body of one Note, saved as typed.
private struct NoteEditor: View {
    @Environment(NotesModel.self) private var model
    let id: String
    @State private var title = ""
    @State private var text = ""
    /// The Note is read; what is typed before is no edit.
    @State private var isLoaded = false

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                TitleField(text: $title, autofocus: model.newID == id)
                MarkdownEditor(text: $text, placeholder: "Note")
                    .accessibilityIdentifier("note-body")
            }
            .padding()
        }
        .onAppear {
            guard !isLoaded, let note = model.note(id: id) else { return }
            (title, text) = (note.title, note.body)
            isLoaded = true
        }
        .onChange(of: title) { save() }
        .onChange(of: text) { save() }
        #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
        #endif
    }

    private func save() {
        guard isLoaded else { return }
        model.save(id: id, title: title, body: text)
    }
}
