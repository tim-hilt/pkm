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
    /// The Task whose sheet is open, over the list shown.
    var editing: ListedTask?
    /// The Tasks planned as `planning`, in their order, each followed by its Subtasks.
    private(set) var tasks: [ListedTask] = []
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
        editing = nil
        guard let id else { return }
        let isTask = { (task: ListedTask) in task.id == id && !task.closed }
        let list = [Planning.active, .backlog].first {
            ((try? store.tasks(planning: $0)) ?? []).contains(where: isTask)
        }
        planning = list ?? .active
        editing = tasks.first(where: isTask)
    }

    func isCollapsed(_ task: ListedTask) -> Bool { collapsed.contains(task.id) }

    /// Hides or shows a Parent's Subtasks.
    func toggleCollapsed(_ task: ListedTask) {
        if !collapsed.insert(task.id).inserted { collapsed.remove(task.id) }
    }

    /// The Close that can still be Reverted, for the toast.
    struct PendingClose: Equatable {
        let title: String
        let closing: Closing
    }
    private(set) var pendingClose: PendingClose?

    /// A Close is saved once its toast has been shown for `revertWindow`.
    init(store: TaskStore, revertWindow: Duration = .seconds(5)) {
        self.store = store
        self.revertWindow = revertWindow
        reload()
    }

    /// Adds a Task and shows the list it landed in. False, adding nothing, if
    /// the Title is empty.
    @discardableResult
    func add(title: String, planning: Planning) -> Bool {
        guard (try? store.create(title: title, planning: planning)) != nil else { return false }
        if self.planning == planning { reload() } else { self.planning = planning }
        return true
    }

    /// Adds a Subtask under `parent` and shows it. False, adding nothing, if the Title is empty.
    @discardableResult
    func addSubtask(title: String, under parent: ListedTask) -> Bool {
        guard (try? store.createSubtask(parent: parent.id, title: title)) != nil else { return false }
        collapsed.remove(parent.id)
        reload()
        return true
    }

    /// Saves a Task's Title, Description, Priority and Tags (adding and removing
    /// what differs). False, saving nothing, if the Title is empty or a Tag is invalid.
    @discardableResult
    func edit(_ task: ListedTask, title: String, description: String, priority: UInt8, tags: [String]) -> Bool {
        guard tags.allSatisfy(isValidTag) else { return false }
        guard (try? store.edit(id: task.id, title: title, description: description, priority: priority)) != nil
        else { return false }
        for tag in task.tags where !tags.contains(tag) { _ = try? store.removeTag(id: task.id, tag: tag) }
        for tag in tags where !task.tags.contains(tag) { _ = try? store.addTag(id: task.id, tag: tag) }
        reload()
        return true
    }

    /// Makes a Task Blocked by exactly the Tasks `ids` (adding and removing what
    /// differs). False if one is refused: itself, its Parent, its Subtask, or a loop.
    @discardableResult
    func setBlockedBy(_ task: ListedTask, to ids: [String]) -> Bool {
        let current = task.blockedBy.map(\.id)
        for id in current where !ids.contains(id) { _ = try? store.removeBlocker(id: task.id, blocker: id) }
        defer { reload() }
        for id in ids where !current.contains(id) {
            guard (try? store.addBlocker(id: task.id, blocker: id)) != nil else { return false }
        }
        return true
    }

    /// The Open Tasks of both lists `task` could be Blocked by: not itself, its
    /// Parent or its Subtasks, and not one it is Blocked by already.
    func blockerCandidates(for task: ListedTask) -> [ListedTask] {
        let blocking = Set(task.blockedBy.map(\.id))
        return [Planning.active, .backlog]
            .flatMap { (try? store.tasks(planning: $0)) ?? [] }
            .filter {
                !$0.closed && !$0.done && $0.id != task.id && $0.id != task.parent && $0.parent != task.id
                    && !blocking.contains($0.id)
            }
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
        pendingClose = PendingClose(title: task.title, closing: closing)
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

    /// The known Tags starting with `prefix`, to pick from.
    func knownTags(prefix: String) -> [String] {
        (try? store.tagsWithPrefix(prefix: prefix)) ?? []
    }

    /// `text` without the characters a Tag can never contain.
    static func allowedTagText(_ text: String) -> String { stripForbiddenTagCharacters(text: text) }

    /// Sync wrote or deleted Task files: the list shows what is there now.
    func syncDidChange() { reload() }

    private func reload() {
        tasks = (try? store.tasks(planning: planning)) ?? []
    }
}

extension ListedTask: Identifiable {}
