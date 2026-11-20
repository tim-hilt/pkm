import CoreTransferable
import SwiftUI
import UniformTypeIdentifiers

/// The Notes Context: the Folder tree, the selected Folder's Notes (newest changed first) and the
/// open Note's editor, side by side where there is room (Mac, iPad); on an iPhone each is pushed
/// over the last.
struct NotesView: View {
    @Environment(NotesModel.self) private var model
    @Environment(\.horizontalSizeClass) private var sizeClass

    var body: some View {
        if NotesLayout.pushes(sizeClass) {
            FolderTree()
                .addNoteButton()
                .navigationDestination(for: String.self) { route in
                    if route.hasPrefix("/") {
                        NoteList().navigationTitle(FolderTree.name(ofPath: String(route.dropFirst())))
                            .addNoteButton()
                    } else {
                        NoteEditor(id: route).id(route)
                    }
                }
        } else {
            HStack(spacing: 0) {
                FolderTree().frame(width: 220)
                Divider()
                NoteList().frame(width: 300)
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

/// A Note or Folder being Moved: dragged, or chosen for "Move to…".
enum NotesMovable: Codable, Hashable, Transferable, Identifiable {
    case note(String)
    case folder(String)

    var id: Self { self }

    static var transferRepresentation: some TransferRepresentation {
        CodableRepresentation(contentType: .json)
    }
}

extension NotesModel {
    /// Moves what is dropped or chosen into the Folder at `folder`; throws a `FolderNameError` for a
    /// Folder Move the core refuses.
    func move(_ item: NotesMovable, into folder: String) throws {
        switch item {
        case .note(let id): moveNote(id: id, into: folder)
        case .folder(let path): try moveFolder(path, into: folder)
        }
    }
}

enum NotesLayout {
    /// Whether the columns are pushed over each other rather than shown side by side. Then the
    /// lists have no selection: a selectable List takes a `NavigationLink(value:)`'s tap as a
    /// selection of the value and pushes nothing.
    static func pushes(_ sizeClass: UserInterfaceSizeClass?) -> Bool { sizeClass == .compact }
}

/// The Folders as an outline under the top level, which is selectable too. New Folder in the
/// toolbar and context menu, Rename and Delete in the context menu, Return renames on the Mac.
private struct FolderTree: View {
    @Environment(NotesModel.self) private var model
    @Environment(\.horizontalSizeClass) private var sizeClass
    @State private var naming: Naming?
    @State private var name = ""
    @State private var problem: FolderNameError?
    @State private var deleting: Folder?
    @State private var moving: NotesMovable?
    /// Why the last drop couldn't be Moved.
    @State private var refusedDrop: FolderNameError?

    /// What the name field of the alert is for.
    private enum Naming: Equatable {
        case create(inside: String)
        case rename(String)

        var title: LocalizedStringKey { if case .create = self { "New Folder" } else { "Rename Folder" } }
        var button: LocalizedStringKey { if case .create = self { "Create" } else { "Rename" } }
    }

    static func name(ofPath path: String) -> String {
        path.isEmpty ? String(localized: "Notes") : String(path.split(separator: "/").last ?? "")
    }

    var body: some View {
        List(selection: NotesLayout.pushes(sizeClass) ? nil : selection) {
            row(path: "", title: "Notes", systemImage: "tray")
            OutlineGroup(model.tree, id: \.path, children: \.children) { folder in
                row(path: folder.path, title: folder.name, systemImage: "folder")
            }
        }
        .toolbar {
            ToolbarItem {
                Button("New Folder", systemImage: "folder.badge.plus") { ask(.create(inside: model.folderPath)) }
                    .accessibilityIdentifier("add-folder")
            }
        }
        #if os(macOS)
            .onKeyPress(.return) {
                guard !model.folderPath.isEmpty else { return .ignored }
                ask(.rename(model.folderPath))
                return .handled
            }
        #endif
        .alert(
            naming?.title ?? "", isPresented: Binding(get: { naming != nil }, set: { if !$0 { naming = nil } }),
            presenting: naming
        ) { target in
            TextField("Name", text: $name)
            Button(target.button) { commit(target) }
            Button("Cancel", role: .cancel) {}
        } message: { _ in
            if let problem { Text(Self.reason(problem)) }
        }
        .confirmationDialog(
            deleting.map { "Delete “\($0.name)”?" } ?? "",
            isPresented: Binding(get: { deleting != nil }, set: { if !$0 { deleting = nil } }),
            presenting: deleting
        ) { folder in
            Button("Delete", role: .destructive) { model.deleteFolder(folder.path) }
        } message: { folder in
            Text(model.summary(ofFolder: folder.path))
        }
        .alert(
            "Can't Move", isPresented: Binding(get: { refusedDrop != nil }, set: { if !$0 { refusedDrop = nil } }),
            presenting: refusedDrop
        ) { _ in
            Button("OK") {}
        } message: {
            Text(Self.reason($0))
        }
        .sheet(item: $moving) { MoveSheet(item: $0) }
        .accessibilityIdentifier("folder-tree")
    }

    private var selection: Binding<String?> {
        Binding(get: { model.folderPath }, set: { model.folderPath = $0 ?? "" })
    }

    private func row(path: String, title: String, systemImage: String) -> some View {
        Group {
            if NotesLayout.pushes(sizeClass) {
                NavigationLink(value: NotesModel.route(to: path)) { Label(title, systemImage: systemImage) }
            } else {
                Label(title, systemImage: systemImage)
            }
        }
        .tag(path)
        .modifier(FolderDrag(path: path) { refusedDrop = $0 })
        .contextMenu {
            Button("New Folder", systemImage: "folder.badge.plus") { ask(.create(inside: path)) }
            if !path.isEmpty {
                Button("Rename", systemImage: "pencil") { ask(.rename(path)) }
                Button("Move to…", systemImage: "folder") { moving = .folder(path) }
                Button("Delete", systemImage: "trash", role: .destructive) { deleting = model.folder(at: path) }
            }
        }
    }

    private func ask(_ target: Naming) {
        problem = nil
        if case .rename(let path) = target { name = Self.name(ofPath: path) } else { name = "" }
        naming = target
    }

    /// Applies the name; a refused one asks again, with the reason.
    private func commit(_ target: Naming) {
        do {
            switch target {
            case .create(let parent): try model.addFolder(named: name, in: parent)
            case .rename(let path): try model.renameFolder(path, to: name)
            }
        } catch let error as FolderNameError {
            problem = error
            Task { naming = target }
        } catch {}
    }

    fileprivate static func reason(_ error: FolderNameError) -> LocalizedStringKey {
        switch error {
        case .Empty: "The name is empty."
        case .HasSlash: "The name can't contain a /."
        case .Hidden: "The name can't start with a dot."
        case .Taken: "A Folder of that name is already there."
        case .Inside: "A Folder can't be moved into itself."
        case .Failed(let message): LocalizedStringKey(message)
        }
    }
}

/// Makes a Folder row draggable and a drop target; the top level (empty path) can't be dragged.
private struct FolderDrag: ViewModifier {
    @Environment(NotesModel.self) private var model
    let path: String
    /// Called with why a dropped Folder can't be Moved here.
    let refused: (FolderNameError) -> Void

    func body(content: Content) -> some View {
        content
            .draggableIf(!path.isEmpty, NotesMovable.folder(path))
            .dropDestination(for: NotesMovable.self) { items, _ in
                var moved = false
                for item in items {
                    do {
                        try model.move(item, into: path)
                        moved = true
                    } catch let error as FolderNameError {
                        refused(error)
                    } catch {}
                }
                return moved
            }
    }
}

/// The "+" for a Note in the selected Folder.
private struct AddNoteButton: ViewModifier {
    @Environment(NotesModel.self) private var model

    func body(content: Content) -> some View {
        content.addButton("New Note", identifier: "add-note") { model.add() }
    }
}

extension View {
    fileprivate func addNoteButton() -> some View { modifier(AddNoteButton()) }

    @ViewBuilder fileprivate func draggableIf(_ condition: Bool, _ item: NotesMovable) -> some View {
        if condition { draggable(item) } else { self }
    }
}

/// The Folder tree to choose where to Move a Note or Folder; a refused Folder Move says why.
private struct MoveSheet: View {
    @Environment(NotesModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let item: NotesMovable
    @State private var problem: FolderNameError?

    var body: some View {
        NavigationStack {
            List {
                if let problem { Text(FolderTree.reason(problem)).foregroundStyle(.red) }
                destination(path: "", title: "Notes", systemImage: "tray")
                OutlineGroup(model.tree, id: \.path, children: \.children) { folder in
                    destination(path: folder.path, title: folder.name, systemImage: "folder")
                }
            }
            .navigationTitle("Move to…")
            #if os(iOS)
                .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } } }
        }
        .frame(minWidth: 280, minHeight: 320)
    }

    private func destination(path: String, title: String, systemImage: String) -> some View {
        Button {
            do {
                try model.move(item, into: path)
                dismiss()
            } catch let error as FolderNameError {
                problem = error
            } catch {}
        } label: {
            Label(title, systemImage: systemImage)
        }
        .buttonStyle(.plain)
    }
}

/// The Notes of the selected Folder. A Note is Deleted by swipe, context menu or ⌫ on the Mac,
/// asking first when its Body has text.
private struct NoteList: View {
    @Environment(NotesModel.self) private var model
    @Environment(\.horizontalSizeClass) private var sizeClass
    @State private var deleting: ListedNote?
    @State private var moving: NotesMovable?
    @FocusState private var isSearchFocused: Bool

    private var isSearching: Bool { !model.searchText.isEmpty }

    var body: some View {
        List(selection: NotesLayout.pushes(sizeClass) ? nil : Bindable(model).openID) {
            if isSearching {
                ForEach(model.results, id: \.id) { result in
                    Group {
                        if NotesLayout.pushes(sizeClass) {
                            NavigationLink(value: result.id) { ResultRow(result: result) }
                        } else {
                            ResultRow(result: result)
                        }
                    }
                    .tag(result.id)
                }
            }
            ForEach(isSearching ? [] : model.rows, id: \.id) { row in
                Group {
                    if NotesLayout.pushes(sizeClass) {
                        NavigationLink(value: row.id) { NoteRow(row: row) }
                    } else {
                        NoteRow(row: row)
                    }
                }
                .tag(row.id)
                .draggable(NotesMovable.note(row.id))
                .swipeActions { Button("Delete", role: .destructive) { ask(row) } }
                .contextMenu {
                    Button("Move to…", systemImage: "folder") { moving = .note(row.id) }
                    Button("Delete", systemImage: "trash", role: .destructive) { ask(row) }
                }
            }
        }
        .searchable(text: Bindable(model).searchText)
        .searchFocused($isSearchFocused)
        #if os(iOS)
            .searchToolbarBehavior(.minimize)
        #endif
        .toolbar {
            #if os(iOS)
                // At the top, so that the bottom stays free for the "+", as in Books.
                DefaultToolbarItem(kind: .search, placement: .topBarTrailing)
            #endif
        }
        .overlay {
            if isSearching {
                if model.results.isEmpty {
                    ContentUnavailableView.search(text: model.searchText)
                }
            } else if model.rows.isEmpty {
                ContentUnavailableView("No Notes", systemImage: "note.text", description: Text("Tap + to write one."))
            }
        }
        #if os(macOS)
            .onDeleteCommand {
                if let id = model.openID, let row = model.rows.first(where: { $0.id == id }) { ask(row) }
            }
        #endif
        .confirmationDialog(
            "Delete this Note?", isPresented: Binding(get: { deleting != nil }, set: { if !$0 { deleting = nil } }),
            presenting: deleting
        ) { row in
            Button("Delete", role: .destructive) { model.delete(id: row.id) }
        }
        .sheet(item: $moving) { MoveSheet(item: $0) }
        .accessibilityIdentifier("notes-list")
    }

    /// Deletes the Note, or asks first if its Body has text.
    private func ask(_ row: ListedNote) {
        if model.confirmsDelete(id: row.id) { deleting = row } else { model.delete(id: row.id) }
    }
}

/// A Search result: the Title, the Folder, and the snippet with the match emphasized.
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
            Label(FolderTree.name(ofPath: result.folder), systemImage: "folder")
                .font(.caption).foregroundStyle(.secondary)
        }
    }

    /// The snippet as typed, with the match in bold; the range counts UTF-16 units.
    private var snippet: AttributedString {
        var text = AttributedString(result.snippet)
        let units = result.snippet.utf16
        if let from = units.index(units.startIndex, offsetBy: Int(result.matchedStart), limitedBy: units.endIndex),
            let to = units.index(units.startIndex, offsetBy: Int(result.matchedEnd), limitedBy: units.endIndex),
            let range = Range(NSRange(from..<to, in: result.snippet), in: text)
        {
            text[range].font = .subheadline.bold()
        }
        return text
    }
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
                MarkdownEditor(text: $text, placeholder: "Note", images: model.images(for: id))
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
        // Sync may have merged another Device's edits in: shown, the next keystroke keeps them.
        .onChange(of: model.syncs) {
            guard isLoaded, let note = model.note(id: id) else { return }
            if note.title != title { title = note.title }
            if note.body != text { text = note.body }
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
