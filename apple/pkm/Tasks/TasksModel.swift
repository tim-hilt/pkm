import Foundation
import Observation

/// The Tasks Context's state: the list being shown, from the core's store.
@MainActor @Observable
final class TasksModel {
    private let store: TaskStore
    private let revertWindow: Duration
    private var commitTimer: Task<Void, Never>?

    /// The list shown: Active or the Backlog.
    var planning: Planning = .active {
        didSet { reload() }
    }
    /// The Tasks of the open sheet's stack, by id, first opened first: a Blocker
    /// or Subtask opened from a Task is pushed on it. Empty when no sheet is open. Over the
    /// list shown, which opening a Blocker never changes.
    private(set) var editing: [String] = []
    /// Whether the sheet can't be left or popped: the Task on top was last
    /// saved with an empty Title, which keeps the stack as it is until it has one.
    private(set) var isLeavingBlocked = false
    /// The Tasks planned as `planning`, in their order, each followed by its Subtasks.
    private(set) var tasks: [ListedTask] = []
    /// Every known Tag with its Tag Color, alphabetically, whether or not a Task carries it.
    private(set) var allTags: [TagEntry] = []

    /// Opens the sheet of the Open Task a link or address names, over the list it
    /// is in, as the only one in the stack. `nil` opens no sheet; an unknown or
    /// Closed `id` shows the Active list.
    func openLinked(id: String?) {
        closeEditing()
        guard let id else { return }
        let isTask = { (task: ListedTask) in task.id == id }
        let list = [Planning.active, .backlog].first {
            ((try? store.tasks(planning: $0)) ?? []).contains(where: isTask)
        }
        planning = list ?? .active
        if let task = tasks.first(where: isTask) { startEditing(task) }
    }

    /// Opens the sheet of an Open `task` of the list shown, as the only one in the stack.
    func startEditing(_ task: ListedTask) {
        editing = [task.id]
        isLeavingBlocked = false
    }

    /// Pushes the Open Task `id`, a Blocker or Subtask of the one on top, onto the stack,
    /// leaving the list shown as it is. False, pushing nothing, if it is Closed or gone,
    /// or while the Title on top is empty.
    @discardableResult
    func open(id: String) -> Bool {
        guard !editing.isEmpty, !isLeavingBlocked, task(id: id) != nil else { return false }
        editing.append(id)
        return true
    }

    /// Pops the Task on top. False, popping nothing, while its Title is empty or it is the last.
    @discardableResult
    func back() -> Bool {
        guard editing.count > 1, !isLeavingBlocked else { return false }
        editing.removeLast()
        return true
    }

    /// Closes the sheet, whatever the stack's depth.
    func closeEditing() {
        editing = []
        isLeavingBlocked = false
    }

    /// The Task `id` as last saved, from either list.
    func task(id: String) -> ListedTask? {
        if let task = tasks.first(where: { $0.id == id }) { return task }
        let other: Planning = planning == .active ? .backlog : .active
        return ((try? store.tasks(planning: other)) ?? []).first { $0.id == id }
    }

    /// The direct Subtasks of the Open Task `id`, in list order: Open, or Done and still
    /// Revertable; never Rejected. From either list.
    func subtasks(of id: String) -> [ListedTask] {
        _ = tasks  // reloads change it: views asking for Subtasks update with the list
        return (try? store.subtasks(parent: id)) ?? []
    }

    /// The Close that can still be Reverted, for the toast.
    private(set) var pendingClose: Closing?

    /// A Close is saved once its toast has been shown for `revertWindow`.
    init(store: TaskStore, revertWindow: Duration = .seconds(5)) {
        self.store = store
        self.revertWindow = revertWindow
        reload()
    }

    /// Adds a Task, with a Description, Priority and Tags if given, and shows the list it landed in.
    /// False, adding nothing, if the Title is empty or a Tag invalid.
    @discardableResult
    func add(
        title: String, description: String = "", planning: Planning = .active, priority: UInt8 = 4, tags: [String] = []
    ) -> Bool {
        guard let task = try? store.create(title: title, planning: planning, priority: priority, tags: tags) else {
            return false
        }
        describe(task, description)
        if self.planning == planning { reload() } else { self.planning = planning }
        return true
    }

    /// Adds a Subtask under `parent`, with a Description, Priority and Tags if given, and shows it.
    /// False, adding nothing, if the Title is empty or a Tag invalid.
    @discardableResult
    func addSubtask(
        title: String, description: String = "", priority: UInt8 = 4, tags: [String] = [], under parent: ListedTask
    ) -> Bool {
        guard
            let task = try? store.createSubtask(parent: parent.id, title: title, priority: priority, tags: tags)
        else { return false }
        describe(task, description)
        reload()
        return true
    }

    /// Gives a Task just created its Description, if any.
    private func describe(_ task: ListedTask, _ description: String) {
        guard !description.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }
        _ = try? store.edit(
            id: task.id, title: task.title, description: description, priority: task.priority, addTags: [])
    }

    /// Saves a Task's Title, Description, Priority and Tags (adding and removing
    /// what differs from what is saved now, not from `task`, which may be stale).
    /// An empty Title keeps the last saved one and saves the rest. False, saving
    /// nothing, if a Tag is invalid. The store writes only what differs, so saving twice is quiet.
    @discardableResult
    func edit(_ task: ListedTask, title: String, description: String, priority: UInt8, tags: [String]) -> Bool {
        guard tags.allSatisfy(isValidTag) else { return false }
        let saved = current(task)
        let isEmpty = title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        if task.id == editing.last { isLeavingBlocked = isEmpty }
        let title = isEmpty ? saved.title : title
        let added = tags.filter { tag in !saved.tags.contains { $0.name == tag } }
        guard
            (try? store.edit(
                id: task.id, title: title, description: description, priority: priority, addTags: added)) != nil
        else { return false }
        for tag in saved.tags.map(\.name) where !tags.contains(tag) { _ = try? store.removeTag(id: task.id, tag: tag) }
        reload()
        return true
    }

    /// Commits Quick Add's Shortcuts to a Task being edited: its Title without them, a Priority
    /// Shortcut replacing `priority`, its Tags added to `tags` (never removed), saved in one write
    /// like `edit`. Returns the Task as saved, or nil, changing nothing, if only Shortcuts were typed
    /// or the save failed.
    func commit(
        _ quickAdd: TaskQuickAdd, to task: ListedTask, description: String, priority: UInt8, tags: [String]
    ) -> ListedTask? {
        guard !quickAdd.title().isEmpty else { return nil }
        let merged = tags + quickAdd.tags().filter { !tags.contains($0) }
        guard
            edit(
                task, title: quickAdd.title(), description: description, priority: quickAdd.priority() ?? priority,
                tags: merged)
        else { return nil }
        return self.task(id: task.id)
    }

    /// Makes a Task Blocked by exactly the Tasks `ids` (adding and removing what
    /// differs from what is saved now). False if one is refused, which `blockerCandidates` never offers.
    @discardableResult
    func setBlockedBy(_ task: ListedTask, to ids: [String]) -> Bool {
        let current = self.current(task).blockedBy.map(\.id)
        for id in current where !ids.contains(id) { _ = try? store.removeBlocker(id: task.id, blocker: id) }
        defer { reload() }
        for id in ids where !current.contains(id) {
            guard (try? store.addBlocker(id: task.id, blocker: id)) != nil else { return false }
        }
        return true
    }

    /// The Open Tasks of both lists `task` could be Blocked by (see the store).
    func blockerCandidates(for task: ListedTask) -> [ListedTask] {
        (try? store.blockerCandidates(id: task.id)) ?? []
    }

    /// Makes `task` a Subtask of `parent`, or top-level with nil. A Parent in the other list takes the
    /// Task there (see the store). False if refused, which `parentCandidates` never offers.
    @discardableResult
    func setParent(_ task: ListedTask, to parent: ListedTask?) -> Bool {
        defer { reload() }
        return (try? store.setParent(id: task.id, parent: parent?.id)) != nil
    }

    /// The Open Tasks of both lists that can be `task`'s Parent (see the store).
    func parentCandidates(for task: ListedTask) -> [ListedTask] {
        (try? store.parentCandidates(id: task.id)) ?? []
    }

    /// Makes a Task Active or moves it to the Backlog; it leaves the list shown.
    func plan(_ task: ListedTask, as planning: Planning) {
        guard (try? store.plan(id: task.id, planning: planning)) != nil else { return }
        reload()
    }

    /// Moves the row at `source` to land before the row at `destination` of `tasks`
    /// (`onMove`'s meaning); past the last row, to the end of the list. The Parent stays.
    func move(from source: IndexSet, to destination: Int) {
        guard let index = source.first, index != destination else { return }
        let target = destination < tasks.count ? tasks[destination] : nil
        _ = try? store.moveBefore(id: tasks[index].id, before: target?.id)
        reload()
    }

    /// Moves the Subtask at `source` of `subtasks(of: parent)` to land before the one at
    /// `destination` (`onMove`'s meaning); past the last, after it. It stays among its siblings.
    func moveSubtask(of parent: String, from source: IndexSet, to destination: Int) {
        let siblings = subtasks(of: parent)
        guard let index = source.first, index != destination else { return }
        let target = destination < siblings.count ? siblings[destination] : nil
        _ = try? store.moveSubtask(id: siblings[index].id, before: target?.id)
        reload()
    }

    /// Closes a Task, committing the previous Close first. Done leaves it in
    /// place, marked, Rejected removes it; either way nothing is saved until
    /// `commit`, which follows `revertWindow` later.
    func close(_ task: ListedTask, as closing: Closing) {
        commitTimer?.cancel()
        guard (try? store.close(id: task.id, closing: closing)) != nil else {
            pendingClose = nil
            reload()
            return
        }
        pendingClose = closing
        reload()
        commitTimer = Task { [revertWindow] in
            try? await Task.sleep(for: revertWindow)
            if !Task.isCancelled { commit() }
        }
    }

    /// The circle of a Task: Done if it is Open, Reverts the Close if it is Done.
    func toggleDone(_ task: ListedTask) {
        if task.done { revert() } else { close(task, as: .done) }
    }

    /// Takes the pending Close back, as if it never happened.
    func revert() {
        commitTimer?.cancel()
        store.revert()
        pendingClose = nil
        reload()
    }

    /// Saves the pending Close, if any; a Done Task leaves the list.
    func commit() {
        commitTimer?.cancel()
        try? store.commit()
        pendingClose = nil
        reload()
    }

    /// Gives a Tag the Tag Color `color` (`#rrggbb`), or Default for nil; the lists show it at once.
    func setTagColor(_ tag: String, to color: String?) {
        try? store.setTagColor(tag: tag, color: color)
        reload()
    }

    /// Deletes a Tag: it is Removed from every Task, Open or Closed, and its Tag Color is forgotten.
    func deleteTag(_ tag: String) {
        try? store.deleteTag(tag: tag)
        reload()
    }

    /// The Tag Color of a Tag by name; Default if it isn't known.
    func tagEntry(_ name: String) -> TagEntry {
        allTags.first { $0.name == name } ?? TagEntry(name: name, color: nil)
    }

    /// What a Tag field offers for the typed `query`, leaving out the Tags the
    /// Task has: known Tags starting with it, then containing it, and whether to offer Create.
    func suggestTags(_ query: String, leaveOut: [String]) -> TagSuggestions {
        (try? store.suggestTags(query: query, leaveOut: leaveOut)) ?? TagSuggestions(matches: [], create: false)
    }

    /// `text` without the characters a Tag can never contain.
    static func allowedTagText(_ text: String) -> String { stripForbiddenTagCharacters(text: text) }

    /// Sync wrote or deleted Task files: the list shows what is there now.
    func syncDidChange() { reload() }

    /// `task` as last saved, if it is in either list; else as given.
    private func current(_ task: ListedTask) -> ListedTask {
        self.task(id: task.id) ?? task
    }

    private func reload() {
        tasks = (try? store.tasks(planning: planning)) ?? []
        allTags = store.tags()
    }
}

extension ListedTask: Identifiable {}
