import CoreTransferable
import SwiftUI
import UniformTypeIdentifiers

/// The Notes Context: the tree of Folders and Notes beside the open Note's editor where there is
/// room (Mac, iPad); on an iPhone the top level's Folders and Notes, each Folder and Note pushed
/// over the last.
struct NotesView: View {
    @Environment(NotesModel.self) private var model
    @Environment(\.horizontalSizeClass) private var sizeClass

    var body: some View {
        if NotesLayout.pushes(sizeClass) {
            NotesBrowser(folder: "")
                .addNoteButton()
                .navigationDestination(for: String.self) { route in
                    if route.hasPrefix("/") {
                        NotesBrowser(folder: String(route.dropFirst()))
                            .navigationTitle(NotesBrowser.name(ofPath: String(route.dropFirst())))
                            .addNoteButton()
                    } else {
                        NoteEditor(id: route).id(route)
                    }
                }
        } else {
            HStack(spacing: 0) {
                NotesBrowser(folder: nil).frame(width: 300)
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
    /// Whether the Folders and the editor are pushed over each other rather than shown side by
    /// side. Then the lists have no selection: a selectable List takes a `NavigationLink(value:)`'s tap as a
    /// selection of the value and pushes nothing.
    static func pushes(_ sizeClass: UserInterfaceSizeClass?) -> Bool { sizeClass == .compact }
}

/// What a row asks of the browser around it, which holds the alerts and sheets.
private enum NotesRequest {
    case newFolder(in: String)
    case rename(String)
    case move(NotesMovable)
    case deleteFolder(String)
    case deleteNote(String)
    case refused(FolderNameError)
}

/// A Folder's Folders by name, then its Notes newest changed first. Pushed (`folder` set) it shows
/// that Folder, and a Folder row pushes its own; else the whole tree from the top level, each
/// Folder expanding in place. New Folder in the toolbar and context menu, Rename, Move and Delete
/// in the context menu, Return renames on the Mac. While searching it shows the results instead.
private struct NotesBrowser: View {
    @Environment(NotesModel.self) private var model
    /// The pushed Folder's path, empty for the top level; nil for the tree.
    let folder: String?
    @State private var naming: Naming?
    @State private var name = ""
    @State private var problem: FolderNameError?
    @State private var deletingFolder: Folder?
    @State private var deletingNote: String?
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

    static func name(ofPath path: String) -> String { String(path.split(separator: "/").last ?? "") }

    private var isSearching: Bool { !model.searchText.isEmpty }
    /// The Folder shown, or the top level of the tree.
    private var path: String { folder ?? "" }

    var body: some View {
        withDialogs(list)
            .sheet(item: $moving) { MoveSheet(item: $0) }
            .accessibilityIdentifier("notes-browser")
    }

    private var list: some View {
        List(selection: folder == nil ? Bindable(model).selection : nil) {
            if isSearching {
                ForEach(model.results, id: \.id) { result in
                    Group {
                        if folder == nil {
                            ResultRow(result: result)
                        } else {
                            NavigationLink(value: result.id) { ResultRow(result: result) }
                        }
                    }
                    .tag(result.id)
                }
            } else {
                FolderContents(path: path, isTree: folder == nil, request: handle)
            }
        }
        #if os(iOS)
            .modifier(TapBesideRows(enabled: folder == nil) { model.selection = nil })
        #endif
        .modifier(FolderDrop(path: path) { handle(.refused($0)) })
        .searchable(text: Bindable(model).searchText)
        #if os(iOS)
            .searchToolbarBehavior(.minimize)
        #endif
        .toolbar {
            #if os(iOS)
                // At the top, so that the bottom stays free for the "+", as in Books.
                DefaultToolbarItem(kind: .search, placement: .topBarTrailing)
            #endif
            ToolbarItem {
                Button("New Folder", systemImage: "folder.badge.plus") {
                    ask(.create(inside: folder ?? model.folderPath))
                }
                .accessibilityIdentifier("add-folder")
            }
        }
        .overlay {
            if isSearching {
                if model.results.isEmpty { ContentUnavailableView.search(text: model.searchText) }
            } else if model.folders(in: path).isEmpty && model.notes(in: path).isEmpty {
                ContentUnavailableView("No Notes", systemImage: "note.text", description: Text("Tap + to write one."))
            }
        }
        #if os(macOS)
            .onKeyPress(.return) {
                guard model.openID == nil, !model.folderPath.isEmpty else { return .ignored }
                ask(.rename(model.folderPath))
                return .handled
            }
            .onDeleteCommand {
                if let id = model.openID { handle(.deleteNote(id)) }
            }
        #endif
    }

    /// The alerts that name, confirm and refuse.
    private func withDialogs(_ content: some View) -> some View {
        content
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
                deletingFolder.map { "Delete “\($0.name)”?" } ?? "",
                isPresented: Binding(get: { deletingFolder != nil }, set: { if !$0 { deletingFolder = nil } }),
                presenting: deletingFolder
            ) { folder in
                Button("Delete", role: .destructive) { model.deleteFolder(folder.path) }
            } message: { folder in
                Text(model.summary(ofFolder: folder.path))
            }
            .confirmationDialog(
                "Delete this Note?",
                isPresented: Binding(get: { deletingNote != nil }, set: { if !$0 { deletingNote = nil } }),
                presenting: deletingNote
            ) { id in
                Button("Delete", role: .destructive) { model.delete(id: id) }
            }
            .alert(
                "Can't Move", isPresented: Binding(get: { refusedDrop != nil }, set: { if !$0 { refusedDrop = nil } }),
                presenting: refusedDrop
            ) { _ in
                Button("OK") {}
            } message: {
                Text(Self.reason($0))
            }
    }

    private func handle(_ request: NotesRequest) {
        switch request {
        case .newFolder(let parent): ask(.create(inside: parent))
        case .rename(let path): ask(.rename(path))
        case .move(let item): moving = item
        case .deleteFolder(let path): deletingFolder = model.folder(at: path)
        // A Note with text is only Deleted after asking.
        case .deleteNote(let id): if model.confirmsDelete(id: id) { deletingNote = id } else { model.delete(id: id) }
        case .refused(let error): refusedDrop = error
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

/// The rows of the Folder at `path`: its Folders, then its Notes. In the tree a Folder expands to
/// its own rows; pushed, it links to them.
private struct FolderContents: View {
    @Environment(NotesModel.self) private var model
    let path: String
    let isTree: Bool
    let request: (NotesRequest) -> Void

    var body: some View {
        ForEach(model.folders(in: path), id: \.path) { folder in
            if isTree {
                DisclosureGroup(isExpanded: expanded(folder.path)) {
                    FolderContents(path: folder.path, isTree: true, request: request)
                } label: {
                    folderRow(folder)
                }
            } else {
                NavigationLink(value: NotesModel.route(to: folder.path)) { folderRow(folder) }
            }
        }
        ForEach(model.notes(in: path), id: \.id) { row in
            Group {
                if isTree {
                    NoteRow(row: row)
                } else {
                    NavigationLink(value: row.id) { NoteRow(row: row) }
                }
            }
            .tag(row.id)
            .draggable(NotesMovable.note(row.id))
            .modifier(FolderDrop(path: path) { request(.refused($0)) })
            .swipeActions { Button("Delete", role: .destructive) { request(.deleteNote(row.id)) } }
            .contextMenu {
                Button("Copy Note", systemImage: "doc.on.doc") { model.copy(id: row.id) }
                Button("Move to…", systemImage: "folder") { request(.move(.note(row.id))) }
                Button("Delete", systemImage: "trash", role: .destructive) { request(.deleteNote(row.id)) }
            }
        }
    }

    private func folderRow(_ folder: Folder) -> some View {
        Label(folder.name, systemImage: "folder")
            .tag(NotesModel.route(to: folder.path))
            .draggable(NotesMovable.folder(folder.path))
            .modifier(FolderDrop(path: folder.path) { request(.refused($0)) })
            .contextMenu {
                Button("New Folder", systemImage: "folder.badge.plus") { request(.newFolder(in: folder.path)) }
                Button("Rename", systemImage: "pencil") { request(.rename(folder.path)) }
                Button("Move to…", systemImage: "folder") { request(.move(.folder(folder.path))) }
                Button("Delete", systemImage: "trash", role: .destructive) { request(.deleteFolder(folder.path)) }
            }
    }

    private func expanded(_ path: String) -> Binding<Bool> {
        Binding(
            get: { model.expanded.contains(path) },
            set: { if $0 { model.expanded.insert(path) } else { model.expanded.remove(path) } }
        )
    }
}

#if os(iOS)
    /// Calls `action` for a tap below a List's last row, which a Mac List takes as selecting none
    /// and an iPad List ignores.
    private struct TapBesideRows: ViewModifier {
        let enabled: Bool
        let action: () -> Void
        /// The empty space below the rows: its top from the List's top, and its height.
        @State private var space = (top: CGFloat.zero, height: CGFloat.zero)

        func body(content: Content) -> some View {
            if enabled {
                content
                    .onScrollGeometryChange(for: [CGFloat].self) { scroll in
                        let top = scroll.contentSize.height - scroll.contentOffset.y
                        return [top, scroll.containerSize.height - scroll.contentInsets.bottom - top]
                    } action: {
                        space = ($1[0], max(0, $1[1]))
                    }
                    .overlay(alignment: .top) {
                        Color.clear
                            .contentShape(.rect)
                            .frame(height: space.height)
                            .onTapGesture(perform: action)
                            .padding(.top, space.top)
                            .ignoresSafeArea(edges: .vertical)
                    }
            } else {
                content
            }
        }
    }
#endif

/// Makes a row, or a whole list, a drop target that Moves into the Folder at `path`.
private struct FolderDrop: ViewModifier {
    @Environment(NotesModel.self) private var model
    let path: String
    /// Called with why a dropped Folder can't be Moved here.
    let refused: (FolderNameError) -> Void

    func body(content: Content) -> some View {
        content.dropDestination(for: NotesMovable.self) { items, _ in
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
                if let problem { Text(NotesBrowser.reason(problem)).foregroundStyle(.red) }
                destination(path: "", title: String(localized: "Top Level"), systemImage: "tray")
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

/// A Search result: the Title, the Folder unless at the top level, and the snippet with the match emphasized.
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
            if !result.folder.isEmpty {
                Label(NotesBrowser.name(ofPath: result.folder), systemImage: "folder")
                    .font(.caption).foregroundStyle(.secondary)
            }
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
