import SwiftUI
import UniformTypeIdentifiers

/// The Drawings Context's first screen: a grid of Drawings, last modified
/// first, with a button to create a new one. A card shows the Drawing's thumbnail, which is made when
/// the card comes into view (the grid is lazy) and again after the Drawing
/// changed, with the name and date below it.
/// A card's context menu (right click, long press) renames, duplicates, exports (via the
/// share sheet) and deletes the Drawing; deleting asks first.
struct GalleryView: View {
    let gallery: Gallery

    /// The Drawing being renamed and the name typed so far.
    @State private var renaming: DrawingStore.Entry?
    @State private var newName = ""
    @State private var deleting: DrawingStore.Entry?
    #if os(iOS)
        @Environment(AppNavigation.self) private var navigation
    #else
        @Environment(\.openSettings) private var openSettings
    #endif

    private let columns = [GridItem(.adaptive(minimum: 160, maximum: 260), spacing: 16, alignment: .top)]

    var body: some View {
        ScrollView {
            LazyVGrid(columns: columns, spacing: 16) {
                ForEach(gallery.drawings) { drawing in
                    Button {
                        gallery.openDrawing(itemID: drawing.id)
                    } label: {
                        Card(drawing: drawing, thumbnails: gallery.thumbnails)
                    }
                    .buttonStyle(.plain)
                    .buttonPointer()
                    .accessibilityIdentifier("drawing-\(drawing.name)")
                    // VoiceOver reaches the context menu's actions from the card itself.
                    .accessibilityActions { actions(for: drawing) }
                    .contextMenu { actions(for: drawing) }
                    // On the card, so that the dialog's popover points at it.
                    .confirmationDialog("Delete", isPresented: isDeleting(drawing)) {
                        Button("Delete", role: .destructive) { gallery.deleteDrawing(named: drawing.name) }
                            .accessibilityIdentifier("delete-confirm")
                    }
                }
            }
            .padding()
        }
        .toolbar {
            ToolbarItem(placement: .status) {
                if let text = gallery.syncText {
                    Text(text).font(.footnote).foregroundStyle(.secondary)
                        .accessibilityIdentifier("sync-progress")
                }
            }
            ToolbarItem {
                if gallery.showsSyncWarning {
                    Button("Cloud Backing needs attention", systemImage: "exclamationmark.triangle.fill") {
                        #if os(iOS)
                            navigation.isShowingSettings = true
                        #else
                            openSettings()
                        #endif
                    }
                    .foregroundStyle(.orange)
                    .accessibilityIdentifier("sync-warning")
                }
            }
            #if os(iOS)
                SettingsToolbarItem()
            #endif
        }
        .addButton("New Drawing", identifier: "new-drawing") { gallery.createDrawing() }
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

    private func isDeleting(_ drawing: DrawingStore.Entry) -> Binding<Bool> {
        Binding(get: { deleting == drawing }, set: { if !$0 { deleting = nil } })
    }
}

private struct Card: View {
    let drawing: DrawingStore.Entry
    let thumbnails: Thumbnails
    @State private var thumbnail: Thumbnails.Thumbnail?
    @Environment(\.colorScheme) private var colorScheme
    @Environment(\.scenePhase) private var scenePhase

    /// What a thumbnail is made for; a change of any restarts the task.
    private struct Request: Equatable {
        let drawing: DrawingStore.Entry
        let dark: Bool
        /// Not in the background, where iOS refuses GPU work. The Gallery is
        /// laid out there when the app launches into it.
        let rendering: Bool
    }

    private var request: Request {
        Request(drawing: drawing, dark: colorScheme == .dark, rendering: scenePhase != .background)
    }

    var body: some View {
        GalleryCell(
            name: drawing.name,
            detail: Text(drawing.modified, format: .dateTime.day().month().year().hour().minute()),
            pageAspect: 4 / 3
        ) {
            // The frame has the thumbnail's shape from the start, so that the
            // grid does not move while thumbnails arrive.
            RoundedRectangle(cornerRadius: 12)
                .fill(.quaternary)
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
                // Restarts when the Drawing or the appearance changed (the card
                // never shows the other appearance's thumbnail) and when the app
                // comes to the front; ends when the card leaves the screen.
                .task(id: request) {
                    let made = await thumbnails.thumbnail(
                        for: request.drawing, dark: request.dark, rendering: request.rendering)
                    // A cancelled task gives way to its successor.
                    if !Task.isCancelled { thumbnail = made }
                }
                // The lazy grid keeps the state of cards scrolled past; without
                // this, every thumbnail seen would stay in memory. `thumbnails`
                // still has it in memory for when the card comes back.
                .onDisappear { thumbnail = nil }
        }
    }
}
