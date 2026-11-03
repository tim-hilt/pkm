import SwiftUI

/// The Gallery as a grid of Drawings, last modified first, with a button to
/// create a new one. Thumbnails come later; a card shows the name for now.
/// A card's menu (also its context menu) renames, duplicates and deletes the
/// Drawing; deleting asks first.
struct GalleryView: View {
    let gallery: Gallery

    /// The Drawing being renamed and the name typed so far.
    @State private var renaming: DrawingStore.Entry?
    @State private var newName = ""
    @State private var deleting: DrawingStore.Entry?

    private let columns = [GridItem(.adaptive(minimum: 160, maximum: 260), spacing: 16)]

    var body: some View {
        NavigationStack {
            ScrollView {
                LazyVGrid(columns: columns, spacing: 16) {
                    ForEach(gallery.drawings) { drawing in
                        Button {
                            gallery.openDrawing(named: drawing.name)
                        } label: {
                            Card(drawing: drawing)
                        }
                        .buttonStyle(.plain)
                        .accessibilityIdentifier("drawing-\(drawing.name)")
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
                Button("New Drawing", systemImage: "plus") { gallery.createDrawing() }
                    .accessibilityIdentifier("new-drawing")
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
        // Files and Finder can change the folder while the app is away.
        .onAppear { gallery.reload() }
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

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            RoundedRectangle(cornerRadius: 12)
                .fill(.quaternary)
                .aspectRatio(4 / 3, contentMode: .fit)
            Text(drawing.name).font(.subheadline).lineLimit(2)
            Text(drawing.modified, format: .dateTime.day().month().year().hour().minute())
                .font(.caption).foregroundStyle(.secondary)
        }
        .contentShape(Rectangle())
    }
}
