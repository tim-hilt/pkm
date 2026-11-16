import SwiftUI

/// The Filters list: each Filter with its Query, opening on tap; an empty list offers to create one.
struct FiltersList: View {
    @Environment(TasksModel.self) private var model
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
            List(model.filters) { filter in
                NavigationLink(value: filter.id) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(filter.title)
                        Text(filter.query)
                            .font(.subheadline)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                    }
                }
            }
            .listStyle(.plain)
        }
    }
}

/// Asks for a Title and a Query; the Filter is added at the end of the Filters list. A Query that
/// can't be read shows why, inline, and can't be saved.
struct NewFilterSheet: View {
    @Environment(TasksModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var title = ""
    @State private var query = ""

    private var problem: QueryProblem? { TasksModel.queryProblem(query) }
    private var canSave: Bool {
        !title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && problem == nil
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
                        .onSubmit(save)
                        .accessibilityIdentifier("filter-query")
                } footer: {
                    if let problem, !query.isEmpty {
                        Text(problem.message)
                            .foregroundStyle(.red)
                            .accessibilityIdentifier("filter-query-error")
                    }
                }
            }
            .formStyle(.grouped)
            .navigationTitle("New Filter")
            #if os(iOS)
                .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Add", action: save).disabled(!canSave)
                }
            }
        }
        #if os(macOS)
            .frame(minWidth: 360)
        #endif
        .presentationDetents([.medium, .large])
    }

    private func save() {
        guard canSave, model.addFilter(title: title, query: query) else { return }
        dismiss()
    }
}
