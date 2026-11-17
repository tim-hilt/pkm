import SwiftUI

/// The Tasks Context: the Active list, the Backlog and the Filters, as Todoist-style rows. A Filter
/// opens on top of the Filters list, unless it is a Favorite, which the bar shows directly.
struct TasksView: View {
    @Environment(TasksModel.self) private var model
    @State private var isAdding = false
    @State private var isAddingFilter = false

    /// The Filter open on top of the Filters list; going back leaves it. A Favorite is no part of it.
    private var openFilter: Binding<[String]> {
        Binding(
            get: { if case .filter(let id) = model.list, model.barEntry == .filters { [id] } else { [] } },
            set: { path in
                if let id = path.last {
                    model.list = .filter(id)
                } else if case .filter = model.list {
                    model.list = .filters
                }
            }
        )
    }

    var body: some View {
        @Bindable var model = model
        NavigationStack(path: openFilter) {
            Group {
                switch model.barEntry {
                case .active, .backlog:
                    TaskListView(emptyText: model.list == .active ? "Nothing active." : "The Backlog is empty.")
                case .filter:
                    TaskListView(emptyText: "No Active Tasks match.")
                case .filters:
                    FiltersList { isAddingFilter = true }
                }
            }
            #if os(iOS)
                .navigationBarTitleDisplayMode(.inline)  // slim top bar: the sidebar button, no title
            #endif
            .toolbar {
                // On iPhone and iPad the bar sits in the bottom bar, in thumb reach; the "+"
                // and the toast sit above it, in its safe area. The Mac keeps it in its toolbar.
                ToolbarItem(placement: barPlacement) {
                    TasksBar()
                }
                .sharedBackgroundVisibility(.hidden)  // no capsule round the bar, which has its own
                if model.barEntry == .filters {
                    AddToolbar(title: "New Filter", identifier: "add-filter") { isAddingFilter = true }
                } else {
                    AddToolbar(title: "New Task", identifier: "add-task") { isAdding = true }
                }
            }
            .navigationDestination(for: String.self) { id in
                TaskListView(emptyText: "No Active Tasks match.")
                    .navigationTitle(model.filters.first { $0.id == id }?.title ?? "")
                    #if os(iOS)
                        .navigationBarTitleDisplayMode(.inline)
                    #endif
                    .toolbar { AddToolbar(title: "New Task", identifier: "add-task") { isAdding = true } }
            }
        }
        // On the stack, not on the content that swaps between empty and list when a Task is added.
        .sheet(isPresented: $isAdding) { NewTaskSheet() }
        .sheet(isPresented: $isAddingFilter) { FilterSheet() }
        .sheet(isPresented: Binding(get: { !model.editing.isEmpty }, set: { if !$0 { model.closeEditing() } })) {
            EditTaskSheet()
        }
    }

    /// Where the bar sits: the bottom bar on iPhone and iPad, the toolbar's middle on the Mac.
    private var barPlacement: ToolbarItemPlacement {
        #if os(iOS)
            .bottomBar
        #else
            .principal
        #endif
    }
}

/// The "+" of a list: at the end of the bottom bar on iPhone and iPad, in the toolbar on the Mac.
private struct AddToolbar: ToolbarContent {
    let title: LocalizedStringKey
    let identifier: String
    let action: () -> Void

    var body: some ToolbarContent {
        #if os(iOS)
            ToolbarSpacer(.flexible, placement: .bottomBar)
            ToolbarItem(placement: .bottomBar) { button }
        #else
            ToolbarItem { button }
        #endif
    }

    private var button: some View {
        Button(title, systemImage: "plus", action: action)
            .keyboardShortcut("n", modifiers: [])
            .accessibilityIdentifier(identifier)
    }
}

/// The Tasks of the list shown (`model.tasks`) as rows with their actions, and the toast of a
/// pending Close. Dragging orders Active or the Backlog, or in a Filter only that Filter.
private struct TaskListView: View {
    @Environment(TasksModel.self) private var model
    let emptyText: LocalizedStringKey

    var body: some View {
        Group {
            if model.tasks.isEmpty {
                ContentUnavailableView {
                    AppContext.tasks.label
                } description: {
                    Text(emptyText)
                }
            } else {
                List {
                    ForEach(model.tasks) { task in
                        TaskRow(task: task, onCircle: { model.toggleDone(task) })
                            .listRowSeparator(task.id == model.tasks.first?.id ? .hidden : .automatic, edges: .top)
                            .contentShape(Rectangle())
                            .onTapGesture { model.startEditing(task) }
                            .swipeActions(edge: .trailing) {
                                if !task.done {
                                    Button("Reject", role: .destructive) { model.close(task, as: .rejected) }
                                }
                            }
                            .contextMenu {
                                if !task.done {  // a pending Done is Reverted at its circle
                                    if task.parent == nil {  // a Subtask follows its top-level Task
                                        if task.planning == .backlog {
                                            Button("Make Active", systemImage: "arrow.up.circle") {
                                                model.plan(task, as: .active)
                                            }
                                        } else {
                                            Button("Move to Backlog", systemImage: "tray") {
                                                model.plan(task, as: .backlog)
                                            }
                                        }
                                    }
                                    Button("Reject", systemImage: "xmark.circle", role: .destructive) {
                                        model.close(task, as: .rejected)
                                    }
                                }
                            }
                    }
                    .onMove { model.move(from: $0, to: $1) }
                }
                .listStyle(.plain)
            }
        }
        .safeAreaInset(edge: .bottom) {
            // While the edit sheet is open, its own toast shows instead.
            if let pendingClose = model.pendingClose, model.editing.isEmpty {
                ClosedToast(closing: pendingClose)
            }
        }
        .animation(.default, value: model.pendingClose)
    }
}

/// Tells that a Task was just Closed and offers to Revert it.
private struct ClosedToast: View {
    @Environment(TasksModel.self) private var model
    let closing: Closing

    var body: some View {
        HStack {
            switch closing {
            case .done: Text("Task Done")
            case .rejected: Text("Task Rejected")
            }
            Spacer()
            Button("Revert") { model.revert() }
                .accessibilityIdentifier("revert-close")
        }
        .padding(.horizontal)
        .frame(minHeight: AddButtonMetrics.size)
        .background(.regularMaterial, in: .rect(cornerRadius: 12))
        .padding(.horizontal)
        .transition(.move(edge: .bottom).combined(with: .opacity))
    }
}

/// A Tag's name in its Tag Color, Default being the secondary grey.
private func tagText(_ tag: TagEntry) -> Text {
    Text(tag.name).foregroundStyle(tag.style())
}

/// A symbol and a text, close together, for the line of Tags.
private func marker(_ text: String, symbol: String) -> some View {
    HStack(spacing: 3) {
        Image(systemName: symbol).imageScale(.small)
        Text(text)
    }
}

/// One Task: a circle, coloured by Priority and filled while Done, and beside
/// it the Title, the Description faded on one line, and the Tags fainter still.
/// A Parent shows how many of its Subtasks are Done, a Subtask its Parent's Title, before
/// the Tags. A Blocked Task has a lock at the end. `onCircle` is a tap on the circle.
struct TaskRow: View {
    let task: ListedTask
    /// Whether the Parent's Title is shown as a marker; not where the Parent is already in view.
    var showsParent = true
    let onCircle: () -> Void

    var body: some View {
        HStack(spacing: 12) {
            Button(action: onCircle) {
                Image(systemName: task.done ? "circle.inset.filled" : "circle")
                    .font(.title2)
                    .foregroundStyle(task.priorityColor)
            }
            .buttonStyle(.plain)
            .accessibilityLabel(task.done ? "Revert Done" : "Done")
            VStack(alignment: .leading, spacing: 2) {
                Text(task.title)
                if !task.description.isEmpty {
                    Text(task.description)
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .truncationMode(.tail)
                }
                if task.hasMarkers(showsParent: showsParent) || !task.tags.isEmpty {
                    HStack(spacing: 8) {
                        if task.subtasksTotal > 0 {
                            marker("\(task.subtasksDone)/\(task.subtasksTotal)", symbol: "checklist")
                                .accessibilityLabel("\(task.subtasksDone) of \(task.subtasksTotal) Subtasks Done")
                        }
                        if showsParent, let parentTitle = task.parentTitle {
                            marker(parentTitle, symbol: "arrow.turn.down.right")
                                .lineLimit(1)
                                .truncationMode(.tail)
                                .opacity(0.7)
                        }
                        if let first = task.tags.first {
                            // One Text, so that too many Tags are cut off at the end, not each.
                            task.tags.dropFirst().reduce(tagText(first)) { Text("\($0) \(tagText($1))") }
                                .lineLimit(1)
                        }
                    }
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                }
            }
            if task.blocked {
                Spacer()
                Image(systemName: "lock.fill")
                    .foregroundStyle(.secondary)
                    .accessibilityLabel("Blocked")
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)  // the whole row opens the Task, not just its text
    }
}

extension ListedTask {
    fileprivate func hasMarkers(showsParent: Bool) -> Bool {
        subtasksTotal > 0 || (showsParent && parentTitle != nil)
    }

    fileprivate var priorityColor: Color {
        switch priority {
        case 1: .red
        case 2: .orange
        case 3: .blue
        default: .gray
        }
    }
}

/// Asks for a Title and a Description; the Task is added Active. The sheet stays open after each
/// Add, cleared and focused for the next Task; Cancel or swiping down ends it.
private struct NewTaskSheet: View {
    @Environment(TasksModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var quickAdd = TaskQuickAdd()
    /// Quick Add's Title, kept up to date by the field.
    @State private var title = ""
    @State private var description = ""
    /// Changes per Task, so that the field redraws empty; it keeps its focus, and so the keyboard.
    @State private var revision = 0

    var body: some View {
        NavigationStack {
            Form {
                QuickAddField(quickAdd: quickAdd, title: $title, revision: revision, onSubmit: add)
                    .accessibilityIdentifier("task-title")
                TextField("Description", text: $description, axis: .vertical)
                    .multilineTextAlignment(.leading)
                    .lineLimit(3...8)
                    .accessibilityIdentifier("task-description")
            }
            .formStyle(.grouped)
            .navigationTitle("New Task")
            #if os(iOS)
                .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Add", action: add).disabled(title.isEmpty)
                }
            }
        }
        #if os(macOS)
            .frame(minWidth: 360)
        #endif
        .presentationDetents([.medium, .large])
    }

    private func add() {
        let priority = quickAdd.priority() ?? 4
        let tags = quickAdd.tags()
        guard model.add(title: title, description: description, priority: priority, tags: tags) else { return }
        quickAdd = TaskQuickAdd()
        title = ""
        description = ""
        revision += 1
    }
}

/// The sheet over the list: a stack of Tasks following the model's, the Task
/// opened first at the bottom and each Blocker or Subtask opened from one pushed on it.
/// While the Title on top is empty the sheet can't be left.
private struct EditTaskSheet: View {
    @Environment(TasksModel.self) private var model
    /// Dismisses the sheet, not a pushed form; its binding then empties the model's stack.
    @Environment(\.dismiss) private var dismiss

    /// The pushed Tasks; popping (Back, edge swipe) goes through the model.
    private var pushed: Binding<[String]> {
        Binding(
            get: { Array(model.editing.dropFirst()) },
            set: { path in
                if path.count > model.editing.count - 1, let id = path.last {
                    model.open(id: id)
                } else {
                    while model.editing.count > path.count + 1, model.back() {}
                }
            }
        )
    }

    @ViewBuilder private func form(_ id: String?) -> some View {
        // Its own identity per Task: a link may swap the bottom Task while the sheet stays.
        if let task = id.flatMap(model.task(id:)) { EditTaskForm(task: task, done: { dismiss() }).id(task.id) }
    }

    var body: some View {
        NavigationStack(path: pushed) {
            form(model.editing.first)
                .navigationDestination(for: String.self) { form($0) }
        }
        .safeAreaInset(edge: .bottom) {
            if let pendingClose = model.pendingClose { ClosedToast(closing: pendingClose) }
        }
        .animation(.default, value: model.pendingClose)
        .interactiveDismissDisabled(model.isLeavingBlocked)
        #if os(macOS)
            .frame(minWidth: 360)
        #endif
        .presentationDetents([.medium, .large])
    }
}

/// Edits a Task, saving as it goes: Title and Description after a pause in typing
/// and when their field loses focus, everything else at once; the Tags are picked in their own
/// sheet. The Title is Quick Add: the pause saves it without pending Shortcuts, and Return, losing focus and leaving the view apply them. While the Title is empty the rest still
/// saves (the last saved Title stays), a hint shows, and the view can't be left.
/// On macOS `done` closes the whole sheet. An Open Blocker or a Subtask is a row that opens its
/// own form on top; Back then shows this one as last saved.
private struct EditTaskForm: View {
    @Environment(TasksModel.self) private var model
    let task: ListedTask
    let done: () -> Void
    @State private var title: String
    /// The Title field's text with its Shortcuts; `title` is what it leaves of it.
    @State private var quickAdd: TaskQuickAdd
    @State private var quickAddRevision = 0
    @State private var description: String
    @State private var priority: UInt8
    @State private var tags: [String]
    @State private var isPickingTags = false
    @State private var blockedBy: [Blocker]
    @FocusState private var focus: Field?

    private enum Field { case description }

    /// How long typing pauses before text is saved.
    private static let pause = Duration.milliseconds(500)

    init(task: ListedTask, done: @escaping () -> Void) {
        self.task = task
        self.done = done
        _title = State(initialValue: task.title)
        _quickAdd = State(initialValue: .fromTitle(title: task.title))
        _description = State(initialValue: task.description)
        _priority = State(initialValue: task.priority)
        _tags = State(initialValue: task.tags.map(\.name))
        _blockedBy = State(initialValue: task.blockedBy)
    }

    /// Open Tasks it could be Blocked by, not yet chosen here.
    private var blockerCandidates: [ListedTask] {
        let chosen = Set(blockedBy.map(\.id))
        return model.blockerCandidates(for: task).filter { !chosen.contains($0.id) }
    }

    private var hasTitle: Bool { !title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }

    /// What the typing pause waits on.
    private var text: [String] { [title, description] }

    var body: some View {
        Form {
            QuickAddField(
                quickAdd: quickAdd, title: $title, revision: quickAddRevision, taskTags: tags, autofocus: false,
                onSubmit: commitQuickAdd, onBlur: commitQuickAdd
            )
            .accessibilityIdentifier("task-title")
            if !hasTitle {
                Text("A Task needs a Title.")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            }
            TextField("Description", text: $description, axis: .vertical)
                .multilineTextAlignment(.leading)
                .lineLimit(3...8)
                .focused($focus, equals: .description)
                .accessibilityIdentifier("task-description")
            Picker("Priority", selection: $priority) {
                ForEach(UInt8(1)...4, id: \.self) { Text(verbatim: "P\($0)").tag($0) }
            }
            .pickerStyle(.segmented)
            .onChange(of: priority) {
                quickAdd.ignorePriority()  // the last action wins
                quickAddRevision += 1
                save()
            }
            TagsRow(tags: tags) { isPickingTags = true }
            Section("Blocked by") {
                ForEach(blockedBy, id: \.id) { blocker in
                    Group {
                        if blocker.closed {
                            Text(blocker.title).strikethrough().foregroundStyle(.secondary)
                        } else {
                            NavigationLink(blocker.title, value: blocker.id).disabled(!hasTitle)
                        }
                    }
                    .swipeActions(edge: .trailing) {
                        Button("Remove", role: .destructive) {
                            blockedBy.removeAll { $0.id == blocker.id }
                            saveBlockedBy()
                        }
                    }
                }
                if !blockerCandidates.isEmpty {
                    Menu("Add Blocked by") {
                        ForEach(blockerCandidates) { candidate in
                            Button(candidate.title) {
                                blockedBy.append(Blocker(id: candidate.id, title: candidate.title, closed: false))
                                saveBlockedBy()
                            }
                        }
                    }
                    .accessibilityIdentifier("task-add-blocker")
                }
            }
            SubtasksSection(task: task, canOpen: hasTitle)
        }
        .formStyle(.grouped)
        .navigationTitle(task.title)  // as last saved; also the back button's label on the Blocker's form
        #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
        #endif
        .toolbar {
            #if os(macOS)
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done", action: done).disabled(!hasTitle)
                }
            #endif
        }
        .onChange(of: focus) { save() }
        .onChange(of: hasTitle) { save() }  // keeps the model's Back and Blocker rules current
        .task(id: text) {
            guard (try? await Task.sleep(for: Self.pause)) != nil else { return }
            save()
        }
        .onDisappear {
            commitQuickAdd()
            save()
        }
        .sheet(isPresented: $isPickingTags) {
            TagPicker(selected: tags) {
                tags = $0
                isPickingTags = false
                save()
            }
        }
        .navigationBarBackButtonHidden(!hasTitle)
        .onAppear(perform: refresh)
    }

    /// Shows what is saved now: this Task may have been edited from a Blocker's form on top.
    private func refresh() {
        guard let saved = model.task(id: task.id) else { return }
        (title, description, priority, tags, blockedBy) =
            (saved.title, saved.description, saved.priority, saved.tags.map(\.name), saved.blockedBy)
        startQuickAdd(from: saved.title)
    }

    private func startQuickAdd(from title: String) {
        quickAdd = .fromTitle(title: title)
        quickAddRevision += 1
    }

    /// Applies the Shortcuts in the Title field: they leave the field, the Priority picker and the
    /// Tag list show the result. With none pending it just saves.
    private func commitQuickAdd() {
        guard quickAdd.priority() != nil || !quickAdd.tags().isEmpty,
            let saved = model.commit(quickAdd, to: task, description: description, priority: priority, tags: tags)
        else { return save() }
        (title, priority, tags) = (saved.title, saved.priority, saved.tags.map(\.name))
        startQuickAdd(from: saved.title)
    }

    /// Saves what is edited, if the Tags are valid.
    private func save() {
        model.edit(task, title: title, description: description, priority: priority, tags: tags)
    }

    private func saveBlockedBy() {
        model.setBlockedBy(task, to: blockedBy.map(\.id))
    }
}
