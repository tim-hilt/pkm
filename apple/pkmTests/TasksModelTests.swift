import Foundation
import SwiftUI
import Testing

@testable import pkm

@MainActor @Suite struct TasksModelTests {
    let root = FileManager.default.temporaryDirectory
        .appending(path: UUID().uuidString, directoryHint: .isDirectory)

    func openModel() throws -> TasksModel {
        TasksModel(
            store: try TaskStore.open(
                root: root.appending(path: "Tasks").path, deviceFolder: root.appending(path: "Device").path))
    }

    @Test func aTaskIsAddedWithTheTitleAndPriorityOfQuickAdd() throws {
        let model = try openModel()
        let quickAdd = TaskQuickAdd()
        quickAdd.replace(start: 0, end: 0, with: "Fix login p1 ")
        #expect(model.add(title: quickAdd.title(), priority: quickAdd.priority() ?? 4, tags: quickAdd.tags()))
        #expect(model.tasks.map(\.title) == ["Fix login"])
        #expect(model.tasks.map(\.priority) == [1])
    }

    @Test func aTaskIsAddedWithTheTagsOfQuickAddCreatingNewOnes() throws {
        let model = try openModel()
        let quickAdd = TaskQuickAdd()
        quickAdd.replace(start: 0, end: 0, with: "Fix login @bug @ui/nav @bug")
        #expect(model.add(title: quickAdd.title(), priority: quickAdd.priority() ?? 4, tags: quickAdd.tags()))
        #expect(model.tasks.map(\.title) == ["Fix login"])
        #expect(model.tasks[0].tags.map(\.name) == ["bug", "ui/nav"])
        #expect(model.allTags.map(\.name) == ["bug", "ui/nav"])
    }

    @Test func aSubtaskIsAddedWithThePriorityOfQuickAdd() throws {
        let model = try openModel()
        #expect(model.add(title: "Parent"))
        let quickAdd = TaskQuickAdd()
        quickAdd.replace(start: 0, end: 0, with: "p2 Child")
        #expect(
            model.addSubtask(
                title: quickAdd.title(), priority: quickAdd.priority() ?? 4, tags: quickAdd.tags(),
                under: model.tasks[0]
            ))
        #expect(model.tasks.map(\.title) == ["Parent", "Child"])
        #expect(model.tasks.map(\.priority) == [4, 2])
    }

    @Test func aTitleOfOnlyShortcutsIsRefusedLikeAnEmptyOne() throws {
        let model = try openModel()
        let quickAdd = TaskQuickAdd()
        quickAdd.replace(start: 0, end: 0, with: "p1")
        #expect(!model.add(title: quickAdd.title(), priority: quickAdd.priority() ?? 4))
        #expect(model.tasks.isEmpty)
    }

    @Test func everyTagIsListedAlphabeticallyEvenOneNoTaskCarriesAnyMore() throws {
        let model = try openModel()
        #expect(model.allTags.isEmpty)
        #expect(model.add(title: "Buy milk", planning: .active))
        let task = model.tasks[0]
        model.edit(task, title: "Buy milk", description: "", priority: 4, tags: ["work/x", "home"])
        model.edit(task, title: "Buy milk", description: "", priority: 4, tags: ["work/x"])
        #expect(model.allTags.map(\.name) == ["home", "work/x"])
    }

    @Test func aTagColorShowsInTheOpenListAtOnceAndDefaultResetsIt() throws {
        let model = try openModel()
        #expect(model.add(title: "Buy milk", planning: .active))
        model.edit(model.tasks[0], title: "Buy milk", description: "", priority: 4, tags: ["home", "work"])
        #expect(model.tasks[0].tags.map(\.color) == [nil, nil])
        model.setTagColor("work", to: "#d0021b")
        #expect(model.tasks[0].tags.map(\.color) == [nil, "#d0021b"])
        #expect(model.allTags.map(\.color) == [nil, "#d0021b"])
        #expect(try openModel().allTags.map(\.color) == [nil, "#d0021b"])
        model.setTagColor("work", to: nil)
        #expect(model.tasks[0].tags.map(\.color) == [nil, nil])
    }

    @Test func deletingATagRemovesItFromEveryTaskAndForgetsItsColor() throws {
        let model = try openModel()
        #expect(model.add(title: "Buy milk", planning: .active))
        model.edit(model.tasks[0], title: "Buy milk", description: "", priority: 4, tags: ["home", "work"])
        model.setTagColor("work", to: "#d0021b")
        model.deleteTag("work")
        #expect(model.tasks[0].tags.map(\.name) == ["home"])
        #expect(model.allTags.map(\.name) == ["home"])
        model.edit(model.tasks[0], title: "Buy milk", description: "", priority: 4, tags: ["home", "work"])
        #expect(model.allTags.map(\.color) == [nil, nil])
        #expect(try openModel().allTags.map(\.name) == ["home", "work"])
    }

    @Test func cancellingTheDeleteConfirmationChangesNothing() throws {
        let model = try openModel()
        #expect(model.add(title: "Buy milk", planning: .active))
        model.edit(model.tasks[0], title: "Buy milk", description: "", priority: 4, tags: ["work"])
        var delete = TagDeletion()
        delete.ask(for: "work")
        #expect(delete.pending == "work")
        delete.cancel()
        #expect(delete.pending == nil)
        #expect(model.allTags.map(\.name) == ["work"])
        #expect(model.tasks[0].tags.map(\.name) == ["work"])
    }

    @Test func aTagColorRoundTripsThroughAColor() {
        #expect(Color(tagColor: "#d0021b")?.tagColor == "#d0021b")
        #expect(Color(tagColor: "red") == nil)
    }

    @Test func aNewTaskAppearsInTheListItWasAddedTo() throws {
        let model = try openModel()
        #expect(model.add(title: "Buy milk", planning: .active))
        #expect(model.tasks.map(\.title) == ["Buy milk"])
    }

    @Test func aTaskThatSyncWroteAppearsOnceTheListIsToldSoAndOneItDeletedLeaves() throws {
        let model = try openModel()
        let file = root.appending(path: "Tasks/aaaaaaaaaaaa.md")
        try
            "---\nid: aaaaaaaaaaaa\ntitle: From afar\npriority: 4\nstate: active\norder: 1\nupdated: 2024-01-01T00:00:00Z\n---\n"
            .write(to: file, atomically: true, encoding: .utf8)
        #expect(model.tasks.isEmpty)
        model.syncDidChange()
        #expect(model.tasks.map(\.title) == ["From afar"])
        try FileManager.default.removeItem(at: file)
        model.syncDidChange()
        #expect(model.tasks.isEmpty)
    }

    @Test func aTaskMovesBetweenTheBacklogAndActive() throws {
        let model = try openModel()
        model.add(title: "Later", planning: .backlog)
        model.plan(model.tasks[0], as: .active)
        #expect(model.tasks.isEmpty)
        model.planning = .active
        #expect(model.tasks.map(\.title) == ["Later"])
        model.plan(model.tasks[0], as: .backlog)
        #expect(model.tasks.isEmpty)
    }

    @Test func aDoneTaskStaysMarkedUntilCommittedThenLeavesAndStays() throws {
        let model = try openModel()
        model.add(title: "Buy milk", planning: .active)
        model.toggleDone(model.tasks[0])
        #expect(model.tasks.map(\.done) == [true])
        #expect(model.pendingClose == .done)
        #expect(try openModel().tasks.map(\.title) == ["Buy milk"])  // nothing saved yet
        model.commit()
        #expect(model.tasks.isEmpty && model.pendingClose == nil)
        #expect(try openModel().tasks.isEmpty)
    }

    @Test func tappingTheFilledCircleReverts() throws {
        let model = try openModel()
        model.add(title: "Buy milk", planning: .active)
        model.toggleDone(model.tasks[0])
        model.toggleDone(model.tasks[0])
        #expect(model.tasks.map(\.done) == [false] && model.pendingClose == nil)
        model.commit()
        #expect(try openModel().tasks.map(\.title) == ["Buy milk"])
    }

    @Test func aRejectedTaskLeavesAtOnceAndRevertBringsItBack() throws {
        let model = try openModel()
        model.add(title: "Oops", planning: .active)
        model.close(model.tasks[0], as: .rejected)
        #expect(model.tasks.isEmpty && model.pendingClose == .rejected)
        model.revert()
        #expect(model.tasks.map(\.title) == ["Oops"])
    }

    @Test func aCloseIsCommittedWhenItsToastGoesAway() async throws {
        let model = TasksModel(
            store: try TaskStore.open(
                root: root.appending(path: "Tasks").path, deviceFolder: root.appending(path: "Device").path),
            revertWindow: .milliseconds(20))
        model.add(title: "Gone", planning: .active)
        model.close(model.tasks[0], as: .rejected)
        try await Task.sleep(for: .milliseconds(300))
        #expect(model.pendingClose == nil)
        #expect(try openModel().tasks.isEmpty)
    }

    @Test func addingToTheBacklogShowsTheBacklog() throws {
        let model = try openModel()
        model.add(title: "Later", planning: .backlog)
        #expect(model.planning == .backlog)
        #expect(model.tasks.map(\.title) == ["Later"])
        model.planning = .active
        #expect(model.tasks.isEmpty)
    }

    @Test func addsATaskWithADescription() throws {
        let model = try openModel()
        #expect(model.add(title: "Buy milk", description: "Two litres"))
        #expect(model.tasks.map(\.description) == ["Two litres"])
        #expect(model.addSubtask(title: "Oat", description: "Barista", under: model.tasks[0]))
        #expect(model.tasks.map(\.description) == ["Two litres", "Barista"])
    }

    @Test func anEmptyTitleAddsNothing() throws {
        let model = try openModel()
        #expect(!model.add(title: "  \n", planning: .active))
        #expect(model.tasks.isEmpty)
    }

    @Test func tasksStayAcrossLaunches() throws {
        try openModel().add(title: "Kept", planning: .active)
        #expect(try openModel().tasks.map(\.title) == ["Kept"])
    }

    @Test func openingBlockersStacksTasksAndBackPopsThem() throws {
        let model = try openModel()
        model.add(title: "b", planning: .active)
        model.add(title: "a", planning: .active)
        let (a, b) = (model.tasks[1], model.tasks[0])
        model.setBlockedBy(a, to: [b.id])
        model.startEditing(model.tasks[1])
        #expect(model.openBlocker(id: model.tasks[1].blockedBy[0].id))
        #expect(model.editing == [a.id, b.id])
        model.back()
        #expect(model.editing == [a.id])
        model.closeEditing()
        #expect(model.editing.isEmpty)
    }

    @Test func aBlockerOnTheOtherListOpensWithoutChangingTheListShown() throws {
        let model = try openModel()
        model.add(title: "far", planning: .backlog)
        model.add(title: "a", planning: .active)
        model.planning = .backlog
        let farTask = model.tasks[0]
        model.planning = .active
        model.setBlockedBy(model.tasks[0], to: [farTask.id])
        model.startEditing(model.tasks[0])
        #expect(model.openBlocker(id: model.tasks[0].blockedBy[0].id))
        #expect(model.editing.last == farTask.id)
        #expect(model.planning == .active)
    }

    @Test func aClosedBlockerAndAnEmptyTitleRefuseOpening() throws {
        let model = try openModel()
        model.add(title: "b", planning: .active)
        model.add(title: "a", planning: .active)
        let (a, b) = (model.tasks[1], model.tasks[0])
        model.setBlockedBy(a, to: [b.id])
        model.startEditing(a)
        model.edit(a, title: " ", description: "", priority: 4, tags: [])
        #expect(!model.openBlocker(id: b.id))
        model.edit(a, title: "a", description: "", priority: 4, tags: [])
        #expect(model.openBlocker(id: b.id))
        model.back()
        model.close(model.tasks[0], as: .rejected)
        model.commit()
        #expect(!model.openBlocker(id: b.id))
        #expect(model.editing == [a.id])
    }

    @Test func backIsRefusedWhileTheTitleOnTopIsEmptyAndShowsTheLatestSavedState() throws {
        let model = try openModel()
        model.add(title: "b", planning: .active)
        model.add(title: "a", planning: .active)
        let (a, b) = (model.tasks[1], model.tasks[0])
        model.setBlockedBy(a, to: [b.id])
        model.startEditing(a)
        model.openBlocker(id: model.tasks[1].blockedBy[0].id)
        model.edit(b, title: "", description: "", priority: 4, tags: [])
        model.back()
        #expect(model.editing == [a.id, b.id])
        model.edit(b, title: "b2", description: "", priority: 4, tags: [])
        model.back()
        #expect(model.task(id: a.id)?.blockedBy.map(\.title) == ["b2"])
    }

    @Test func aBlockerOnTheOtherListIsSavedToo() throws {
        let model = try openModel()
        model.add(title: "far", planning: .backlog)
        model.add(title: "a", planning: .active)
        model.planning = .backlog
        let far = model.tasks[0]
        model.planning = .active
        model.setBlockedBy(model.tasks[0], to: [far.id])
        model.edit(far, title: "far2", description: "", priority: 4, tags: [])
        #expect(model.task(id: far.id)?.title == "far2")
    }

    @Test func editingChangesTheRowAndStaysAcrossLaunches() throws {
        let model = try openModel()
        model.add(title: "Buy milk", planning: .active)
        #expect(model.edit(model.tasks[0], title: "Buy oat milk", description: "Two litres", priority: 1, tags: []))
        #expect(model.tasks.map(\.title) == ["Buy oat milk"])
        #expect(model.tasks.map(\.description) == ["Two litres"])
        #expect(model.tasks.map(\.priority) == [1])
        #expect(try openModel().tasks.map(\.priority) == [1])
    }

    @Test func committingQuickAddAppliesTheShortcutsAndLeavesThemOutOfTheTitle() throws {
        let model = try openModel()
        model.add(title: "Fix login", priority: 3, tags: ["home"])
        let quickAdd = TaskQuickAdd.fromTitle(title: "Fix login")
        quickAdd.replace(start: 9, end: 9, with: " p1 @bug")
        let saved = model.commit(quickAdd, to: model.tasks[0], description: "", priority: 3, tags: ["home"])
        #expect(saved?.title == "Fix login")
        #expect(saved?.priority == 1)
        #expect(saved?.tags.map(\.name) == ["home", "bug"])
    }

    @Test func aSavedTitleStaysAsItIsWhenCommitted() throws {
        let model = try openModel()
        model.add(title: "Ship p1 @bug", priority: 3)
        let quickAdd = TaskQuickAdd.fromTitle(title: "Ship p1 @bug")
        let saved = model.commit(quickAdd, to: model.tasks[0], description: "", priority: 3, tags: [])
        #expect(saved?.title == "Ship p1 @bug")
        #expect(saved?.priority == 3)
        #expect(saved?.tags.isEmpty == true)
    }

    @Test func thePickerOverridesAPendingPriorityShortcutButALaterOneWins() throws {
        let model = try openModel()
        model.add(title: "Fix", priority: 4)
        let quickAdd = TaskQuickAdd.fromTitle(title: "Fix")
        quickAdd.replace(start: 3, end: 3, with: " p1")
        quickAdd.ignorePriority()  // the picker was used
        var saved = model.commit(quickAdd, to: model.tasks[0], description: "", priority: 3, tags: [])
        #expect(saved?.title == "Fix p1")
        #expect(saved?.priority == 3)
        quickAdd.replace(start: 6, end: 6, with: " p2 ")
        saved = model.commit(quickAdd, to: model.tasks[0], description: "", priority: 3, tags: [])
        #expect(saved?.title == "Fix p1")
        #expect(saved?.priority == 2)
    }

    @Test func committingOnlyShortcutsChangesNothing() throws {
        let model = try openModel()
        model.add(title: "Fix", priority: 4)
        let quickAdd = TaskQuickAdd.fromTitle(title: "")
        quickAdd.replace(start: 0, end: 0, with: "p1 @bug ")
        #expect(model.commit(quickAdd, to: model.tasks[0], description: "", priority: 4, tags: []) == nil)
        #expect(model.tasks[0].priority == 4)
    }

    @Test func editingToAnEmptyTitleKeepsTheOldTitleAndSavesTheRest() throws {
        let model = try openModel()
        model.add(title: "Keep", planning: .active)
        #expect(model.edit(model.tasks[0], title: " ", description: "x", priority: 1, tags: []))
        #expect(model.tasks.map(\.title) == ["Keep"])
        #expect(model.tasks.map(\.description) == ["x"])
        #expect(try openModel().tasks.map(\.title) == ["Keep"])
    }

    @Test func anEditKeepsTheLastSavedTitleEvenFromAStaleTask() throws {
        let model = try openModel()
        model.add(title: "Old", planning: .active)
        let stale = model.tasks[0]
        model.edit(stale, title: "New", description: "", priority: 4, tags: ["a"])
        #expect(model.edit(stale, title: "", description: "", priority: 4, tags: []))
        #expect(model.tasks.map(\.title) == ["New"])
        #expect(model.tasks[0].tags.isEmpty)
    }

    @Test func tagsAreAddedAndRemovedOnSave() throws {
        let model = try openModel()
        model.add(title: "t", planning: .active)
        #expect(model.edit(model.tasks[0], title: "t", description: "", priority: 4, tags: ["home", "work/x"]))
        #expect(model.tasks[0].tags.map(\.name) == ["home", "work/x"])
        #expect(model.edit(model.tasks[0], title: "t", description: "", priority: 4, tags: ["work/x"]))
        #expect(try openModel().tasks[0].tags.map(\.name) == ["work/x"])
    }

    @Test func anInvalidTagSavesNothing() throws {
        let model = try openModel()
        model.add(title: "t", planning: .active)
        #expect(!model.edit(model.tasks[0], title: "new", description: "", priority: 4, tags: ["Bad"]))
        #expect(model.tasks[0].title == "t")
    }

    @Test func knownTagsAreSuggestedByPrefixThenAnywhere() throws {
        let model = try openModel()
        model.add(title: "t", planning: .active)
        model.edit(model.tasks[0], title: "t", description: "", priority: 4, tags: ["home", "work", "network"])
        #expect(model.suggestTags("w", leaveOut: []).matches == ["work", "network"])
        #expect(model.suggestTags("w", leaveOut: ["work"]).matches == ["network"])
        #expect(
            model.suggestTags("", leaveOut: []) == TagSuggestions(matches: ["home", "network", "work"], create: false))
        #expect(model.suggestTags("x", leaveOut: []) == TagSuggestions(matches: [], create: true))
        #expect(!model.suggestTags("work", leaveOut: []).create)
    }

    @Test func forbiddenTagCharactersAreStripped() {
        #expect(TasksModel.allowedTagText("Work 1/x") == "ork1/x")
    }

    @Test func aSubtaskIsListedAfterItsParentOneLevelDeeper() throws {
        let model = try openModel()
        model.add(title: "Parent", planning: .backlog)
        #expect(model.addSubtask(title: "Child", under: model.tasks[0]))
        #expect(!model.addSubtask(title: " ", under: model.tasks[0]))
        #expect(model.tasks.map(\.title) == ["Parent", "Child"])
        #expect(model.tasks.map(\.depth) == [0, 1])
        #expect(model.tasks.map(\.hasSubtasks) == [true, false])
    }

    @Test func collapsingAParentHidesItsWholeSubtree() throws {
        let model = try openModel()
        model.add(title: "Parent", planning: .active)
        model.addSubtask(title: "Child", under: model.tasks[0])
        model.addSubtask(title: "Grandchild", under: model.tasks[1])
        model.add(title: "Other", planning: .active)
        model.toggleCollapsed(model.tasks[0])
        #expect(model.rows.map(\.title) == ["Parent", "Other"])
        model.toggleCollapsed(model.tasks[0])
        #expect(model.rows.map(\.title) == ["Parent", "Child", "Grandchild", "Other"])
    }

    @Test func aSubtaskCannotBeMadeActiveByItself() throws {
        let model = try openModel()
        model.add(title: "Parent", planning: .backlog)
        model.addSubtask(title: "Child", under: model.tasks[0])
        model.plan(model.tasks[1], as: .active)
        #expect(model.tasks.map(\.planning) == [.backlog, .backlog])
    }

    @Test func closingAParentClosesItsSubtasksAndRevertBringsThemBack() throws {
        let model = try openModel()
        model.add(title: "Parent", planning: .active)
        model.addSubtask(title: "Child", under: model.tasks[0])
        model.toggleDone(model.tasks[0])
        #expect(model.tasks.map(\.done) == [true, true])
        model.revert()
        #expect(model.tasks.map(\.done) == [false, false])
    }

    @Test func aClosedSubtaskStaysListedMarkedClosed() throws {
        let model = try openModel()
        model.add(title: "Parent", planning: .active)
        model.addSubtask(title: "Child", under: model.tasks[0])
        model.close(model.tasks[1], as: .rejected)
        model.commit()
        #expect(model.tasks.map(\.closed) == [false, true])
    }

    @Test func aTaskIsDraggedBeforeAnotherAndToTheEnd() throws {
        let model = try openModel()
        for title in ["a", "b", "c"] { model.add(title: title, planning: .active) }
        model.move(from: [2], to: 0)
        #expect(model.tasks.map(\.title) == ["c", "a", "b"])
        model.move(from: [0], to: 3)
        #expect(model.tasks.map(\.title) == ["a", "b", "c"])
        #expect(try openModel().tasks.map(\.title) == ["a", "b", "c"])
    }

    @Test func aTaskIsDroppedUnderAnotherAndOutAgain() throws {
        let model = try openModel()
        for title in ["a", "b"] { model.add(title: title, planning: .active) }
        model.move(model.tasks[1], under: model.tasks[0])
        #expect(model.tasks.map(\.title) == ["a", "b"] && model.tasks.map(\.depth) == [0, 1])
        model.move(from: [1], to: 2)
        #expect(model.tasks.map(\.depth) == [0, 0])
    }

    @Test func aTaskCannotBeDroppedUnderItsOwnSubtask() throws {
        let model = try openModel()
        model.add(title: "a", planning: .active)
        model.addSubtask(title: "b", under: model.tasks[0])
        model.move(model.tasks[0], under: model.tasks[1])
        #expect(model.tasks.map(\.depth) == [0, 1])
    }

    @Test func aTaskIsBlockedWhileATaskItIsBlockedByIsOpen() throws {
        let model = try openModel()
        for title in ["a", "b"] { model.add(title: title, planning: .active) }
        #expect(model.setBlockedBy(model.tasks[0], to: [model.tasks[1].id]))
        #expect(model.tasks.map(\.blocked) == [true, false])
        #expect(model.tasks[0].blockedBy.map(\.title) == ["b"])
        model.toggleDone(model.tasks[1])
        model.commit()
        #expect(model.tasks.map(\.blocked) == [false])
    }

    @Test func aBlockedByIsRemovedAndRefusedWhenItClosesALoop() throws {
        let model = try openModel()
        for title in ["a", "b"] { model.add(title: title, planning: .active) }
        let (a, b) = (model.tasks[0], model.tasks[1])
        #expect(model.setBlockedBy(a, to: [b.id]))
        #expect(!model.setBlockedBy(model.tasks[1], to: [a.id]))
        #expect(model.setBlockedBy(model.tasks[0], to: []))
        #expect(model.tasks.map(\.blocked) == [false, false])
    }

    @Test func candidatesLeaveOutATaskThatWouldCloseALoop() throws {
        let model = try openModel()
        for title in ["a", "b"] { model.add(title: title, planning: .active) }
        model.setBlockedBy(model.tasks[1], to: [model.tasks[0].id])
        #expect(model.blockerCandidates(for: model.tasks[0]).isEmpty)
    }

    @Test func candidatesLeaveOutTheTaskItsParentItsSubtasksExistingBlockersAndLoops() throws {
        let model = try openModel()
        for title in ["a", "b"] { model.add(title: title, planning: .active) }
        model.add(title: "later", planning: .backlog)
        model.planning = .active
        model.addSubtask(title: "sub", under: model.tasks[0])
        model.setBlockedBy(model.tasks[0], to: [model.tasks[2].id])
        #expect(model.blockerCandidates(for: model.tasks[0]).map(\.title) == ["later"])
        #expect(model.blockerCandidates(for: model.tasks[2]).map(\.title) == ["sub", "later"])
    }
}
