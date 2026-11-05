import SwiftUI
import UniformTypeIdentifiers

/// The Gallery as a grid of Drawings, last modified first, with a button to
/// create a new one. A card shows the Drawing's thumbnail, which is made when
/// the card comes into view (the grid is lazy) and again after the Drawing
/// changed, with the name and date below it.
/// A card's menu (also its context menu) renames, duplicates and deletes the
/// Drawing, exports it via the share sheet and deletes it; deleting asks first. `.excalidraw` files are imported from Files
/// (toolbar) or by dropping them onto the Gallery. The toolbar also opens the
/// third-party licences.
struct GalleryView: View {
    let gallery: Gallery

    /// The Drawing being renamed and the name typed so far.
    @State private var renaming: DrawingStore.Entry?
    @State private var newName = ""
    @State private var deleting: DrawingStore.Entry?
    @State private var isImporting = false

    private let columns = [GridItem(.adaptive(minimum: 160, maximum: 260), spacing: 16)]

    var body: some View {
        NavigationStack {
            ScrollView {
                LazyVGrid(columns: columns, spacing: 16) {
                    ForEach(gallery.drawings) { drawing in
                        Button {
                            gallery.openDrawing(named: drawing.name)
                        } label: {
                            Card(drawing: drawing, thumbnails: gallery.thumbnails)
                        }
                        .buttonStyle(.plain)
                        .accessibilityIdentifier("drawing-\(drawing.name)")
                        // VoiceOver reaches the menu's actions from the card itself.
                        .accessibilityActions { actions(for: drawing) }
                        .overlay(alignment: .topTrailing) {
                            Menu {
                                actions(for: drawing)
                            } label: {
                                Image(systemName: "ellipsis.circle.fill")
                                    .symbolRenderingMode(.hierarchical)
                                    .font(.title2)
                                    .padding(8)
                                    .contentShape(Rectangle())
                            }
                            .accessibilityLabel("Actions for \(drawing.name)")
                            .accessibilityIdentifier("actions-\(drawing.name)")
                        }
                        .contextMenu { actions(for: drawing) }
                    }
                }
                .padding()
            }
            .navigationTitle("Drawings")
            .toolbar {
                NavigationLink {
                    LicensesView()
                } label: {
                    Label("Licences", systemImage: "info.circle")
                }
                .accessibilityIdentifier("licenses")
                Button("Import", systemImage: "square.and.arrow.down") { isImporting = true }
                    .accessibilityIdentifier("import-drawing")
                Button("New Drawing", systemImage: "plus") { gallery.createDrawing() }
                    .accessibilityIdentifier("new-drawing")
            }
            .fileImporter(isPresented: $isImporting, allowedContentTypes: [.excalidraw], allowsMultipleSelection: true)
            { result in
                switch result {
                case .success(let urls): gallery.importDrawings(from: urls)
                case .failure: break  // cancelled or nothing chosen
                }
            }
            .dropDestination(for: URL.self) { urls, _ in
                let files = urls.filter { $0.isFileURL }
                guard !files.isEmpty else { return false }
                gallery.importDrawings(from: files)
                return true
            }
            .overlay {
                if gallery.drawings.isEmpty {
                    ContentUnavailableView(
                        "No Drawings", systemImage: "scribble", description: Text("Tap + to start sketching."))
                }
            }
            .alert(
                gallery.failure ?? "",
                isPresented: Binding(get: { gallery.failure != nil }, set: { if !$0 { gallery.dismissFailure() } })
            ) {}
            .alert("Rename Drawing", isPresented: isRenaming) {
                TextField("Name", text: $newName)
                    .accessibilityIdentifier("rename-field")
                Button("Rename") {
                    if let drawing = renaming { gallery.renameDrawing(named: drawing.name, to: newName) }
                }
                .accessibilityIdentifier("rename-confirm")
                Button("Cancel", role: .cancel) {}
            }
            .confirmationDialog(
                "Delete “\(deleting?.name ?? "")”?", isPresented: isDeleting, titleVisibility: .visible
            ) {
                Button("Delete", role: .destructive) {
                    if let drawing = deleting { gallery.deleteDrawing(named: drawing.name) }
                }
                .accessibilityIdentifier("delete-confirm")
                Button("Cancel", role: .cancel) {}
            } message: {
                Text("This Drawing will be deleted for good.")
            }
        }
        // Files and Finder change the folder while the app is away or the
        // Gallery is not shown; while it is shown, the list follows them.
        .task { await gallery.followChangesFromOutside() }
    }

    @ViewBuilder
    private func actions(for drawing: DrawingStore.Entry) -> some View {
        Button("Rename", systemImage: "pencil") {
            newName = drawing.name
            renaming = drawing
        }
        .accessibilityIdentifier("rename")
        Button("Duplicate", systemImage: "plus.square.on.square") { gallery.duplicateDrawing(named: drawing.name) }
            .accessibilityIdentifier("duplicate")
        ShareLink(item: ExportedDrawing(name: drawing.name, gallery: gallery), preview: SharePreview(drawing.name)) {
            Label("Export", systemImage: "square.and.arrow.up")
        }
        Button("Delete", systemImage: "trash", role: .destructive) { deleting = drawing }
            .accessibilityIdentifier("delete")
    }

    private var isRenaming: Binding<Bool> {
        Binding(get: { renaming != nil }, set: { if !$0 { renaming = nil } })
    }

    private var isDeleting: Binding<Bool> {
        Binding(get: { deleting != nil }, set: { if !$0 { deleting = nil } })
    }
}

private struct Card: View {
    let drawing: DrawingStore.Entry
    let thumbnails: Thumbnails
    @State private var thumbnail: Thumbnails.Thumbnail?

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            // The frame has the thumbnail's shape from the start, so that the
            // grid does not move while thumbnails arrive.
            RoundedRectangle(cornerRadius: 12)
                .fill(.quaternary)
                .aspectRatio(4 / 3, contentMode: .fit)
                .overlay {
                    if let image = thumbnail?.image {
                        Image(decorative: image, scale: 1)
                            .resizable()
                            .aspectRatio(contentMode: .fill)
                    } else if thumbnail?.isUnreadable == true {
                        // A malformed file is shown as such; it is left as it is.
                        Label("Can’t be read", systemImage: "exclamationmark.triangle")
                            .font(.footnote)
                            .foregroundStyle(.secondary)
                            .accessibilityIdentifier("unreadable-\(drawing.name)")
                    }
                }
                .clipShape(RoundedRectangle(cornerRadius: 12))
                .overlay { RoundedRectangle(cornerRadius: 12).strokeBorder(.quaternary) }
                // Restarts when the Drawing changed; ends when the card leaves the screen.
                .task(id: drawing) { thumbnail = await thumbnails.thumbnail(for: drawing) }
                // The lazy grid keeps the state of cards scrolled past; without
                // this, every thumbnail seen would stay in memory. `thumbnails`
                // still has it in memory for when the card comes back.
                .onDisappear { thumbnail = nil }
            Text(drawing.name).font(.subheadline).lineLimit(2)
            Text(drawing.modified, format: .dateTime.day().month().year().hour().minute())
                .font(.caption).foregroundStyle(.secondary)
        }
        .contentShape(Rectangle())
    }
}
