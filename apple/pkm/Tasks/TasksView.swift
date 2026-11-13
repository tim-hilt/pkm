import SwiftUI

/// The Tasks Context: the Active list and the Backlog, as Todoist-style rows.
struct TasksView: View {
    @Environment(TasksModel.self) private var model
    @State private var isAdding = false
    @State private var editing: OpenTask?

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
                    List(model.tasks) { task in
                        TaskRow(task: task)
                            .contentShape(Rectangle())
                            .onTapGesture { editing = task }
                    }
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
            .sheet(item: $editing) { EditTaskSheet(task: $0) }
        }
    }
}

/// One Task: an empty circle, coloured by Priority, and beside it the Title and,
/// faded on one line, the Description.
private struct TaskRow: View {
    let task: OpenTask

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: "circle")
                .font(.title2)
                .foregroundStyle(task.priorityColor)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                Text(task.title)
                if !task.description.isEmpty {
                    Text(task.description)
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .truncationMode(.tail)
                }
            }
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

/// Edits a Task's Title, Description and Priority; Save is off while the Title is empty.
private struct EditTaskSheet: View {
    @Environment(TasksModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let task: OpenTask
    @State private var title: String
    @State private var description: String
    @State private var priority: UInt8

    init(task: OpenTask) {
        self.task = task
        _title = State(initialValue: task.title)
        _description = State(initialValue: task.description)
        _priority = State(initialValue: task.priority)
    }

    private var canSave: Bool { !title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }

    var body: some View {
        NavigationStack {
            Form {
                TextField("Title", text: $title)
                    .onSubmit(save)
                    .accessibilityIdentifier("task-title")
                TextField("Description", text: $description, axis: .vertical)
                    .lineLimit(3...8)
                    .accessibilityIdentifier("task-description")
                Picker("Priority", selection: $priority) {
                    ForEach(UInt8(1)...4, id: \.self) { Text("P\($0)").tag($0) }
                }
                .pickerStyle(.segmented)
            }
            .formStyle(.grouped)
            .navigationTitle("Task")
            #if os(iOS)
                .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save", action: save).disabled(!canSave)
                }
            }
        }
        #if os(macOS)
            .frame(minWidth: 360)
        #endif
        .presentationDetents([.medium])
    }

    private func save() {
        if model.edit(task, title: title, description: description, priority: priority) { dismiss() }
    }
}
