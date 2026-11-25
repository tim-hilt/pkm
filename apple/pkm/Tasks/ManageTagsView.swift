import SwiftUI

/// Every Tag by Relevance, each `a/b` as one flat row written in its Tag Color, its Relevance
/// on the right; "No Tags" if there are none. Swiping a row offers Edit, which picks the Tag
/// Color or resets it to Default, Reset to 0, which resets its Relevance, and Delete, which a
/// full swipe runs.
struct ManageTagsView: View {
    @Environment(TasksModel.self) private var tasks
    @Environment(BookmarksModel.self) private var bookmarks
    @Environment(NotesModel.self) private var notes
    @State private var deletion = TagDeletion()
    @State private var editing: String?

    var body: some View {
        List {
            if !tasks.allTags.isEmpty {
                Section {
                    ForEach(tasks.allTags, id: \.name) { tag in row(tag) }
                } header: {
                    LabeledContent("Tag", value: "Relevance").font(.subheadline)
                }
            }
        }
        .listStyle(.plain)
        .sheet(isPresented: isEditing) {
            if let tag = tasks.allTags.first(where: { $0.name == editing }) {
                TagColorPicker(title: tag.name, color: color(of: tag))
                    .safeAreaInset(edge: .bottom) {
                        Button("Default Color") {
                            tasks.setTagColor(tag.name, to: nil)
                            editing = nil
                        }
                        .disabled(tag.color == nil)
                        .padding()
                    }
            }
        }
        .confirmationDialog(
            "Delete '\(deletion.pending ?? "")'? It is Removed from every Item.",
            isPresented: asking, titleVisibility: .visible
        ) {
            Button("Delete", role: .destructive) {
                if let tag = deletion.pending {
                    bookmarks.removeTagEverywhere(tag)
                    notes.removeTagEverywhere(tag)
                    tasks.deleteTag(tag)  // last: it forgets the Tag
                }
                deletion.cancel()
            }
            Button("Cancel", role: .cancel) { deletion.cancel() }
        }
        .overlay { if tasks.allTags.isEmpty { Text("No Tags").foregroundStyle(.secondary) } }
        .navigationTitle("Tags")
        .onAppear(perform: tasks.refreshTags)
    }

    private func row(_ tag: TagEntry) -> some View {
        LabeledContent {
            Text(tag.relevance, format: .number).monospacedDigit()
        } label: {
            Text(tag.name).foregroundStyle(tag.style(default: .primary))
        }
        .swipeActions(edge: .trailing) {
            Button("Delete", systemImage: "trash", role: .destructive) { deletion.ask(for: tag.name) }
            Button("Edit", systemImage: "pencil") { editing = tag.name }.tint(.accentColor)
            if tag.relevance > 0 {
                Button("Reset to 0", systemImage: "arrow.counterclockwise") {
                    tasks.resetTagRelevance(tag.name)
                }
            }
        }
        .contextMenu {
            Button("Edit", systemImage: "pencil") { editing = tag.name }
            Button("Default Color") { tasks.setTagColor(tag.name, to: nil) }
                .disabled(tag.color == nil)
            Button("Reset to 0", systemImage: "arrow.counterclockwise") { tasks.resetTagRelevance(tag.name) }
                .disabled(tag.relevance == 0)
            Button("Delete", systemImage: "trash", role: .destructive) { deletion.ask(for: tag.name) }
        }
    }

    /// The dialog shows while a Tag is pending; dismissing it cancels.
    private var asking: Binding<Bool> {
        Binding(get: { deletion.pending != nil }, set: { if !$0 { deletion.cancel() } })
    }

    /// The color picker shows while a Tag is being edited.
    private var isEditing: Binding<Bool> {
        Binding(get: { editing != nil }, set: { if !$0 { editing = nil } })
    }

    /// The picker's color: the Tag Color, or the grey of Default.
    private func color(of tag: TagEntry) -> Binding<Color> {
        Binding(
            get: { tag.color.flatMap(Color.init(tagColor:)) ?? .gray },
            set: {
                // The picker reports often; a pick that changes nothing writes nothing.
                let picked = $0.tagColor
                if picked != tag.color { tasks.setTagColor(tag.name, to: picked) }
            })
    }
}

#if os(iOS)
    /// The system color picker as a sheet, titled after the Tag; it writes each
    /// finished pick (not every step of a drag) and closes with its own button.
    private struct TagColorPicker: UIViewControllerRepresentable {
        let title: String
        let color: Binding<Color>
        @Environment(\.dismiss) private var dismiss

        func makeUIViewController(context: Context) -> UIColorPickerViewController {
            let picker = UIColorPickerViewController()
            picker.title = title
            picker.supportsAlpha = false
            picker.selectedColor = UIColor(color.wrappedValue)
            picker.delegate = context.coordinator
            return picker
        }

        func updateUIViewController(_ picker: UIColorPickerViewController, context: Context) {
            context.coordinator.picker = self
        }

        func makeCoordinator() -> Coordinator { Coordinator(picker: self) }

        final class Coordinator: NSObject, UIColorPickerViewControllerDelegate {
            var picker: TagColorPicker

            init(picker: TagColorPicker) { self.picker = picker }

            func colorPickerViewController(
                _ controller: UIColorPickerViewController, didSelect color: UIColor, continuously: Bool
            ) {
                if !continuously { picker.color.wrappedValue = Color(uiColor: color) }
            }

            func colorPickerViewControllerDidFinish(_ controller: UIColorPickerViewController) {
                picker.dismiss()
            }
        }
    }
#else
    /// The system color picker in a sheet titled after the Tag.
    private struct TagColorPicker: View {
        let title: String
        let color: Binding<Color>
        @Environment(\.dismiss) private var dismiss

        var body: some View {
            Form { ColorPicker(title, selection: color, supportsOpacity: false) }
                .padding()
                .toolbar { Button("Done") { dismiss() } }
        }
    }
#endif

/// The Tag awaiting the user's answer to "Delete 'work'?"; none when nobody is asked.
struct TagDeletion {
    private(set) var pending: String?

    mutating func ask(for tag: String) { pending = tag }
    mutating func cancel() { pending = nil }
}
