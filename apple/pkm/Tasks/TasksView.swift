import SwiftUI

/// The Tasks Context: the Active list and the Backlog, as Todoist-style rows.
struct TasksView: View {
    @Environment(TasksModel.self) private var model
    @State private var isAdding = false

    var body: some View {
        @Bindable var model = model
        NavigationStack {
            Group {
                if model.tasks.isEmpty {
                    ContentUnavailableView {
                        AppContext.tasks.label
                    } description: {
                        Text(model.planning == .active ? "Nothing active." : "The Backlog is empty.")
                    }
                } else {
                    List(model.tasks) { TaskRow(task: $0) }
                        .listStyle(.plain)
                }
            }
            .navigationTitle(Text(AppContext.tasks.title))
            .toolbar {
                ToolbarItem(placement: .principal) {
                    Picker("List", selection: $model.planning) {
                        Text("Active").tag(Planning.active)
                        Text("Backlog").tag(Planning.backlog)
                    }
                    .pickerStyle(.segmented)
                    .labelsHidden()
                    .accessibilityIdentifier("tasks-planning")
                }
                #if os(iOS)
                    ToolbarItem(placement: .topBarTrailing) { SettingsButton() }
                #endif
            }
            .addButton("New Task", identifier: "add-task") { isAdding = true }
            .sheet(isPresented: $isAdding) { NewTaskSheet(planning: model.planning) }
        }
    }
}

/// One Task: an empty circle, coloured by Priority, and the Title beside it.
private struct TaskRow: View {
    let task: OpenTask

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: "circle")
                .font(.title2)
                .foregroundStyle(task.priorityColor)
                .accessibilityHidden(true)
            Text(task.title)
        }
        .padding(.vertical, 4)
    }
}

extension OpenTask {
    fileprivate var priorityColor: Color {
        switch priority {
        case 1: .red
        case 2: .orange
        case 3: .blue
        default: .gray
        }
    }
}

/// Asks for a Title and Active or Backlog (Active unless the Backlog is shown).
private struct NewTaskSheet: View {
    @Environment(TasksModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var title = ""
    @State private var planning: Planning
    @FocusState private var isTitleFocused: Bool

    init(planning: Planning) { _planning = State(initialValue: planning) }

    private var canAdd: Bool { !title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }

    var body: some View {
        NavigationStack {
            Form {
                TextField("Title", text: $title)
                    .focused($isTitleFocused)
                    .onSubmit(add)
                    .accessibilityIdentifier("task-title")
                Picker("List", selection: $planning) {
                    Text("Active").tag(Planning.active)
                    Text("Backlog").tag(Planning.backlog)
                }
                .pickerStyle(.segmented)
            }
            .formStyle(.grouped)
            .navigationTitle("New Task")
            #if os(iOS)
                .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Add", action: add).disabled(!canAdd)
                }
            }
        }
        .onAppear { isTitleFocused = true }
        #if os(macOS)
            .frame(minWidth: 360)
        #endif
        .presentationDetents([.medium])
    }

    private func add() {
        if model.add(title: title, planning: planning) { dismiss() }
    }
}
