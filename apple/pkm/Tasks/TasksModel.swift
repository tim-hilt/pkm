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
    /// opened from a Task is pushed on it. Empty when no sheet is open. Over the
    /// list shown, which opening a Blocker never changes.
    private(set) var editing: [String] = []
    /// Whether the sheet can't be left or popped: the Task on top was last
    /// saved with an empty Title, which keeps the stack as it is until it has one.
    private(set) var isLeavingBlocked = false
    /// The Tasks planned as `planning`, in their order, each followed by its Subtasks.
    private(set) var tasks: [ListedTask] = []
    /// Every known Tag with its Tag Color, alphabetically, whether or not a Task carries it.
    private(set) var allTags: [TagEntry] = []
    /// Parents whose Subtasks are hidden.
    private var collapsed: Set<String> = []

    /// `tasks` without the Subtasks of a collapsed Parent.
    var rows: [ListedTask] {
        var hiddenBelow: UInt32?
        return tasks.filter { task in
            if let depth = hiddenBelow {
                if task.depth > depth { return false }
                hiddenBelow = nil
            }
            if collapsed.contains(task.id) { hiddenBelow = task.depth }
            return true
        }
    }

    /// Opens the sheet of the Open Task `id`, over the list it is in. `nil` opens
    /// no sheet; an unknown or Closed `id` shows the Active list.
    func open(id: String?) {
        closeEditing()
        guard let id else { return }
        let isTask = { (task: ListedTask) in task.id == id && !task.closed }
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

    /// Pushes the Open Task `id`, a Blocker of the one on top, onto the stack.
    /// False, pushing nothing, if it is Closed or gone, or while the Title on top is empty.
    @discardableResult
    func openBlocker(id: String) -> Bool {
        guard !editing.isEmpty, !isLeavingBlocked, let blocker = task(id: id), !blocker.closed else { return false }
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

    func isCollapsed(_ task: ListedTask) -> Bool { collapsed.contains(task.id) }

    /// Hides or shows a Parent's Subtasks.
    func toggleCollapsed(_ task: ListedTask) {
        if !collapsed.insert(task.id).inserted { collapsed.remove(task.id) }
    }

    /// The Close that can still be Reverted, for the toast.
    private(set) var pendingClose: Closing?

    /// A Close is saved once its toast has been shown for `revertWindow`.
    init(store: TaskStore, revertWindow: Duration = .seconds(5)) {
        self.store = store
        self.revertWindow = revertWindow
        reload()
    }

    /// Adds a Task, with a Description if given, and shows the list it landed in.
    /// False, adding nothing, if the Title is empty.
    @discardableResult
    func add(title: String, description: String = "", planning: Planning = .active) -> Bool {
        guard let task = try? store.create(title: title, planning: planning) else { return false }
        describe(task, description)
        if self.planning == planning { reload() } else { self.planning = planning }
        return true
    }

    /// Adds a Subtask under `parent`, with a Description if given, and shows it.
    /// False, adding nothing, if the Title is empty.
    @discardableResult
    func addSubtask(title: String, description: String = "", under parent: ListedTask) -> Bool {
        guard let task = try? store.createSubtask(parent: parent.id, title: title) else { return false }
        describe(task, description)
        collapsed.remove(parent.id)
        reload()
        return true
    }

    /// Gives a Task just created its Description, if any.
    private func describe(_ task: ListedTask, _ description: String) {
        guard !description.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }
        _ = try? store.edit(id: task.id, title: task.title, description: description, priority: task.priority)
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
        guard (try? store.edit(id: task.id, title: title, description: description, priority: priority)) != nil
        else { return false }
        for tag in saved.tags.map(\.name) where !tags.contains(tag) { _ = try? store.removeTag(id: task.id, tag: tag) }
        for tag in tags where !saved.tags.contains(where: { $0.name == tag }) {
            _ = try? store.addTag(id: task.id, tag: tag)
        }
        reload()
        return true
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

    /// Makes a Task Active or moves it to the Backlog; it leaves the list shown.
    func plan(_ task: ListedTask, as planning: Planning) {
        guard (try? store.plan(id: task.id, planning: planning)) != nil else { return }
        reload()
    }

    /// Moves the rows at `source` to land before the row at `destination` of `rows`
    /// (`onMove`'s meaning), as a sibling of that row; past the last row, to the end
    /// of the top level. Refused moves, e.g. under the Task's own Subtask, change nothing.
    func move(from source: IndexSet, to destination: Int) {
        let rows = rows
        guard let index = source.first, index != destination else { return }
        let target = destination < rows.count ? rows[destination] : nil
        move(rows[index], under: target?.parent, before: target)
    }

    /// Moves a Task to be the last Subtask of `parent`.
    func move(_ task: ListedTask, under parent: ListedTask) {
        move(task, under: parent.id, before: nil)
    }

    private func move(_ task: ListedTask, under parent: String?, before: ListedTask?) {
        _ = try? store.moveTo(id: task.id, parent: parent, planning: planning, before: before?.id)
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

    /// The known Tags starting with `prefix`, to pick from.
    func knownTags(prefix: String) -> [String] {
        (try? store.tagsWithPrefix(prefix: prefix)) ?? []
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
