import SwiftUI

/// The Filters list: each Filter with its Query and how many Open Tasks it matches, opening on
/// tap and ordered by dragging; swiping a row (a context menu on the Mac) marks it as a Favorite,
/// edits or Deletes it, the latter after a confirmation. An empty list offers to create one.
struct FiltersList: View {
    @Environment(TasksModel.self) private var model
    @Binding var editing: Filter?
    @State private var deleting: Filter?
    let onNew: () -> Void

    var body: some View {
        if model.filters.isEmpty {
            ContentUnavailableView {
                Label("Filters", systemImage: "line.3.horizontal.decrease")
            } description: {
                Text("No Filters yet.")
            } actions: {
                Button("New Filter", action: onNew)
            }
        } else {
            List {
                ForEach(model.filters) { filter in
                    NavigationLink(value: filter.id) { FilterRow(filter: filter) }
                        .swipeActions(edge: .trailing) {
                            Button("Delete", systemImage: "trash", role: .destructive) { deleting = filter }
                            Button("Edit", systemImage: "pencil") { editing = filter }.tint(.blue)
                        }
                        .swipeActions(edge: .leading) { favoriteButton(filter).tint(.yellow) }
                        .contextMenu {
                            favoriteButton(filter)
                            Button("Edit", systemImage: "pencil") { editing = filter }
                            Button("Delete", systemImage: "trash", role: .destructive) { deleting = filter }
                        }
                }
                .onMove { model.moveFilters(from: $0, to: $1) }
            }
            .listStyle(.plain)
            .sheet(item: $editing) { FilterSheet(editing: $0) }
            .confirmationDialog(
                "Delete “\(deleting?.title ?? "")”?",
                isPresented: Binding(get: { deleting != nil }, set: { if !$0 { deleting = nil } }),
                titleVisibility: .visible, presenting: deleting
            ) { filter in
                Button("Delete", role: .destructive) { model.deleteFilter(filter.id) }
                Button("Cancel", role: .cancel) {}
            }
        }
    }
}

extension FiltersList {
    /// Marks the Filter as a Favorite, or unmarks it.
    fileprivate func favoriteButton(_ filter: Filter) -> some View {
        Button(
            filter.favorite ? "Unfavorite" : "Favorite", systemImage: filter.favorite ? "star.slash" : "star"
        ) { model.setFavorite(filter.id, to: !filter.favorite) }
    }
}

/// A Filter's Title, its Query (or why it can't be read), and the count of Open Tasks it matches.
private struct FilterRow: View {
    let filter: Filter

    var body: some View {
        HStack {
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 4) {
                    Text(filter.title)
                    if filter.favorite {
                        Image(systemName: "star.fill").font(.caption).foregroundStyle(.yellow)
                            .accessibilityLabel("Favorite")
                    }
                }
                if let error = filter.error {
                    Label(error, systemImage: "exclamationmark.triangle")
                        .font(.subheadline)
                        .foregroundStyle(.red)
                        .lineLimit(1)
                } else {
                    Text(filter.query)
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
            }
            Spacer()
            if filter.error == nil {
                Text(filter.matches, format: .number)
                    .monospacedDigit()
                    .foregroundStyle(.secondary)
                    .accessibilityLabel("\(filter.matches) Open Tasks")
            }
        }
    }
}

/// Asks for a Title, a Query and a Group by: adds the Filter at the end of the Filters list, or,
/// given one, changes it. A Query that can't be read, or a Group by that isn't a list of existing
/// Tags, shows why, inline, and can't be saved.
struct FilterSheet: View {
    @Environment(TasksModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    private let editing: Filter?
    @State private var title: String
    @State private var query: String
    @State private var groupBy: String

    init(editing: Filter? = nil) {
        self.editing = editing
        _title = State(initialValue: editing?.title ?? "")
        _query = State(initialValue: editing?.query ?? "")
        _groupBy = State(initialValue: editing?.groupBy.map { "@\($0)" }.joined(separator: " ") ?? "")
    }

    private var check: QueryCheck { model.checkQuery(query) }
    private var groupByProblem: String? { model.checkGroupBy(groupBy) }
    private var canSave: Bool {
        !title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && check.problem == nil
            && groupByProblem == nil
    }

    var body: some View {
        NavigationStack {
            Form {
                TextField("Title", text: $title)
                    .accessibilityIdentifier("filter-title")
                Section {
                    TextField("Query", text: $query, prompt: Text(verbatim: "@work & !@in-review"))
                        .autocorrectionDisabled()
                        #if os(iOS)
                            .textInputAutocapitalization(.never)
                        #endif
                        .onChange(of: query) {
                            let typed = TasksModel.allowedQueryText(query)
                            if typed != query { query = typed }
                        }
                        .onSubmit(save)
                        .accessibilityIdentifier("filter-query")
                    ForEach(model.suggestQueryTags(query), id: \.self) { tag in
                        Button {
                            query = TasksModel.completingQuery(query, with: tag)
                        } label: {
                            Label(tag, systemImage: "tag")
                                .foregroundStyle(model.tagEntry(tag).style(default: .primary))
                        }
                    }
                } footer: {
                    queryNotes
                }
                Section {
                    TextField("Group by", text: $groupBy, prompt: Text(verbatim: "@work @private"))
                        .autocorrectionDisabled()
                        #if os(iOS)
                            .textInputAutocapitalization(.never)
                        #endif
                        .onSubmit(save)
                        .accessibilityIdentifier("filter-group-by")
                    ForEach(model.suggestGroupByTags(groupBy), id: \.self) { tag in
                        Button {
                            groupBy = TasksModel.completingQuery(groupBy, with: tag)
                        } label: {
                            Label(tag, systemImage: "tag")
                                .foregroundStyle(model.tagEntry(tag).style(default: .primary))
                        }
                    }
                } footer: {
                    if let problem = groupByProblem {
                        Text(problem)
                            .foregroundStyle(.red)
                            .accessibilityIdentifier("filter-group-by-error")
                    }
                }
            }
            .formStyle(.grouped)
            .navigationTitle(editing == nil ? "New Filter" : "Edit Filter")
            #if os(iOS)
                .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button(editing == nil ? "Add" : "Save", action: save).disabled(!canSave)
                }
            }
        }
        #if os(macOS)
            .frame(minWidth: 360)
        #endif
        .presentationDetents([.medium, .large])
    }

    /// Why the Query can't be saved, or which of its Tags don't exist (which doesn't stop saving).
    @ViewBuilder private var queryNotes: some View {
        let check = check
        if let problem = check.problem, !query.isEmpty {
            Text(problem.message)
                .foregroundStyle(.red)
                .accessibilityIdentifier("filter-query-error")
        } else if !check.unknown.isEmpty {
            Text("Unknown: " + check.unknown.map { "@\($0)" }.joined(separator: ", "))
                .foregroundStyle(.orange)
                .accessibilityIdentifier("filter-query-unknown")
        }
    }

    private func save() {
        guard canSave else { return }
        let saved =
            if let editing {
                model.editFilter(editing.id, title: title, query: query, groupBy: groupBy)
            } else {
                model.addFilter(title: title, query: query, groupBy: groupBy)
            }
        if saved { dismiss() }
    }
}
