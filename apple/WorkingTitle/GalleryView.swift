import SwiftUI

/// The Gallery as a grid of Drawings, last modified first, with a button to
/// create a new one. Thumbnails come later; a card shows the name for now.
struct GalleryView: View {
    let gallery: Gallery

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
        }
        // Files and Finder can change the folder while the app is away.
        .onAppear { gallery.reload() }
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
