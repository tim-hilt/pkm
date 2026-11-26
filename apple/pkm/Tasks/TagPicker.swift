import SwiftUI

/// Every known Tag with a checkbox on the right, and search on top, which also offers
/// to create a Tag; the keyboard opens only when search is tapped. Done hands back the checked Tags;
/// leaving otherwise changes nothing. In single-toggle mode there is no Done: a tap on a Tag, or
/// creating one, hands back that one Tag to `toggle` and the caller closes the picker; Cancel leaves.
struct TagPicker: View {
    @Environment(TasksModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    private let picking: Picking
    @State private var selected: [String]
    /// Tags created here, which are known only once the Task is saved.
    @State private var created: [String] = []
    @State private var query = ""
    @State private var refusedText = false

    private enum Picking {
        case many(done: ([String]) -> Void)
        case one(toggle: (String) -> Void)
    }

    /// Multi-select: `done` gets the checked Tags.
    init(selected: [String], done: @escaping ([String]) -> Void) {
        _selected = State(initialValue: selected)
        picking = .many(done: done)
    }

    /// Single-toggle: `toggle` gets the one Tag tapped or created, then the picker is over.
    init(selected: [String], toggle: @escaping (String) -> Void) {
        _selected = State(initialValue: selected)
        picking = .one(toggle: toggle)
    }

    /// Created and known Tags that contain what is searched; known ones by Relevance.
    private var names: [String] {
        (created + model.allTags.map(\.name)).filter { query.isEmpty || $0.contains(query) }
    }

    private var canCreate: Bool {
        !query.isEmpty && isValidTag(tag: query) && !model.allTags.contains { $0.name == query }
            && !created.contains(query)
    }

    private func toggle(_ tag: String) {
        if case .one(let toggle) = picking { return toggle(tag) }
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
                        if case .one(let toggle) = picking { return toggle(query) }
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
            // iOS 26 crashes presenting the default bottom search bar inside a sheet.
            #if os(iOS)
                .searchable(
                    text: $query, placement: .navigationBarDrawer(displayMode: .always), prompt: "Search or create")
            #else
                .searchable(text: $query, prompt: "Search or create")
            #endif
            #if os(iOS)
                .searchPresentationToolbarBehavior(.avoidHidingContent)
            #endif
            .autocorrectionDisabled()
            #if os(iOS)
                .textInputAutocapitalization(.never)
            #endif
            .onChange(of: query) { _, text in
                let allowed = TasksModel.allowedTagText(text)
                refusedText = allowed != text
                if refusedText { query = allowed }
            }
            .toolbar {
                switch picking {
                case .many(let done):
                    ToolbarItem(placement: .confirmationAction) {
                        Button("Done") { done(selected) }
                    }
                case .one:
                    ToolbarItem(placement: .cancellationAction) {
                        Button("Cancel") { dismiss() }
                    }
                }
            }
        }
        #if os(macOS)
            // A Mac sheet sizes to its content, and a List has no height of its own.
            .frame(minWidth: 320, idealWidth: 360, minHeight: 400, idealHeight: 520)
        #endif
        .presentationDetents([.large])
    }
}
