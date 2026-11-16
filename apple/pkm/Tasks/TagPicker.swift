import SwiftUI

/// The line in a Task's sheet that shows its Tags as pills in their Tag Colors,
/// and opens the picker when tapped.
struct TagsRow: View {
    @Environment(TasksModel.self) private var model
    let tags: [String]
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 6) {
                Image(systemName: "tag").foregroundStyle(.secondary)
                if tags.isEmpty {
                    Text("Add Tags").foregroundStyle(.secondary)
                } else {
                    ForEach(tags, id: \.self) { tag in
                        let style = model.tagEntry(tag).style(default: .secondary)
                        Text(tag)
                            .foregroundStyle(style)
                            .padding(.horizontal, 8)
                            .padding(.vertical, 2)
                            .background(style.opacity(0.18), in: .capsule)
                    }
                }
                Spacer(minLength: 0)
            }
            .lineLimit(1)
            .contentShape(.rect)
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("task-tags")
    }
}

/// Every known Tag with a checkbox on the right, and search at the bottom, which takes the
/// focus on opening and also offers to create a Tag. Done hands back the checked Tags;
/// leaving otherwise changes nothing.
struct TagPicker: View {
    @Environment(TasksModel.self) private var model
    let done: ([String]) -> Void
    @State private var selected: [String]
    /// Tags created here, which are known only once the Task is saved.
    @State private var created: [String] = []
    @State private var query = ""
    @State private var refusedText = false
    /// Search is focused as the picker opens, so typing searches right away.
    @FocusState private var isSearching: Bool

    init(selected: [String], done: @escaping ([String]) -> Void) {
        _selected = State(initialValue: selected)
        self.done = done
    }

    /// Known and created Tags that contain what is searched, in alphabetical order.
    private var names: [String] {
        (model.allTags.map(\.name) + created)
            .filter { query.isEmpty || $0.contains(query) }
            .sorted()
    }

    private var canCreate: Bool {
        !query.isEmpty && isValidTag(tag: query) && !model.allTags.contains { $0.name == query }
            && !created.contains(query)
    }

    private func toggle(_ tag: String) {
        if let index = selected.firstIndex(of: tag) { selected.remove(at: index) } else { selected.append(tag) }
    }

    var body: some View {
        NavigationStack {
            List {
                if refusedText {
                    Text("Tags use a–z, 0–9, - and / only.")
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                }
                if canCreate {
                    Button("Create Tag '\(query)'") {
                        created.append(query)
                        selected.append(query)
                        query = ""
                    }
                }
                ForEach(names, id: \.self) { tag in
                    Button {
                        toggle(tag)
                    } label: {
                        HStack {
                            Text(tag).foregroundStyle(model.tagEntry(tag).style(default: .primary))
                            Spacer()
                            Image(systemName: selected.contains(tag) ? "checkmark.circle.fill" : "circle")
                                .foregroundStyle(
                                    selected.contains(tag) ? AnyShapeStyle(.tint) : AnyShapeStyle(.secondary))
                        }
                        .contentShape(.rect)
                    }
                    .buttonStyle(.plain)
                    .accessibilityAddTraits(selected.contains(tag) ? .isSelected : [])
                }
            }
            .navigationTitle("Tags")
            #if os(iOS)
                .navigationBarTitleDisplayMode(.inline)
            #endif
            .searchable(text: $query, prompt: "Search or create")
            .searchFocused($isSearching)
            // Keeps Done reachable while searching.
            .searchPresentationToolbarBehavior(.avoidHidingContent)
            .onAppear { isSearching = true }
            #if os(iOS)
                .textInputAutocapitalization(.never)
            #endif
            .autocorrectionDisabled()
            .onChange(of: query) { _, text in
                let allowed = TasksModel.allowedTagText(text)
                refusedText = allowed != text
                if refusedText { query = allowed }
            }
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { done(selected) }
                }
                #if os(iOS)
                    DefaultToolbarItem(kind: .search, placement: .bottomBar)
                #endif
            }
        }
        #if os(macOS)
            // A Mac sheet sizes to its content, and a List has no height of its own.
            .frame(minWidth: 320, idealWidth: 360, minHeight: 400, idealHeight: 520)
        #endif
        .presentationDetents([.large])
    }
}
