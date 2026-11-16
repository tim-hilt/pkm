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
        model.list = .active
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
        #expect(model.list == .backlog)
        #expect(model.tasks.map(\.title) == ["Later"])
        model.list = .active
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
        #expect(model.open(id: model.tasks[1].blockedBy[0].id))
        #expect(model.editing == [a.id, b.id])
        model.back()
        #expect(model.editing == [a.id])
        model.closeEditing()
        #expect(model.editing.isEmpty)
    }

    @Test func aSubtaskOpensOnTopOfItsParentAndMixesWithBlockers() throws {
        let model = try openModel()
        model.add(title: "p", planning: .backlog)
        model.list = .backlog
        let p = model.tasks[0]
        model.addSubtask(title: "s", under: p)
        let s = model.subtasks(of: p.id)[0]
        model.startEditing(p)
        #expect(model.open(id: s.id))
        #expect(model.editing == [p.id, s.id])
        #expect(model.list == .backlog)
        model.back()
        #expect(model.editing == [p.id])
    }

    @Test func subtasksAddedInARowKeepTheirOrder() throws {
        let model = try openModel()
        model.add(title: "p")
        let p = model.tasks[0]
        for title in ["one", "two", "three"] { model.addSubtask(title: title, under: p) }
        model.addSubtask(title: "nested", under: model.subtasks(of: p.id)[0])
        model.addSubtask(title: "four", under: p)
        #expect(model.subtasks(of: p.id).map(\.title) == ["one", "two", "three", "four"])
        #expect(model.tasks.map(\.title) == ["p", "one", "nested", "two", "three", "four"])
        #expect(model.editing.isEmpty)
    }

    @Test func subtasksAreTheDirectOpenOnesInListOrderEvenFromTheOtherList() throws {
        let model = try openModel()
        model.add(title: "p", planning: .backlog)
        model.list = .backlog
        let p = model.tasks[0]
        model.addSubtask(title: "rejected", under: p)
        model.addSubtask(title: "deep", under: model.subtasks(of: p.id)[0])
        model.addSubtask(title: "kept", under: p)
        model.close(model.subtasks(of: p.id)[0], as: .rejected)
        model.commit()
        model.list = .active
        #expect(model.subtasks(of: p.id).map(\.title) == ["kept"])
        #expect(model.subtasks(of: p.id).first?.parentTitle == "p")
        #expect(model.subtasks(of: "unknown").isEmpty)
    }

    @Test func aSubtaskIsDoneRevertedAndRejectedInItsParentsSection() throws {
        let model = try openModel()
        model.add(title: "p")
        let p = model.tasks[0]
        for title in ["a", "b"] { model.addSubtask(title: title, under: p) }
        model.addSubtask(title: "deep", under: model.subtasks(of: p.id)[0])

        model.toggleDone(model.subtasks(of: p.id)[0])
        #expect(model.subtasks(of: p.id).map(\.done) == [true, false])
        #expect(model.task(id: p.id)?.subtasksDone == 1 && model.task(id: p.id)?.subtasksTotal == 2)

        model.toggleDone(model.subtasks(of: p.id)[0])
        #expect(model.subtasks(of: p.id).map(\.done) == [false, false])
        #expect(model.task(id: p.id)?.subtasksDone == 0)

        model.close(model.subtasks(of: p.id)[0], as: .rejected)
        #expect(model.subtasks(of: p.id).map(\.title) == ["b"])
        #expect(model.task(id: p.id)?.subtasksTotal == 1)
        #expect(model.tasks.map(\.title) == ["p", "b"])  // "a" and its Subtask "deep" went together
    }

    @Test func aBlockerOnTheOtherListOpensWithoutChangingTheListShown() throws {
        let model = try openModel()
        model.add(title: "far", planning: .backlog)
        model.add(title: "a", planning: .active)
        model.list = .backlog
        let farTask = model.tasks[0]
        model.list = .active
        model.setBlockedBy(model.tasks[0], to: [farTask.id])
        model.startEditing(model.tasks[0])
        #expect(model.open(id: model.tasks[0].blockedBy[0].id))
        #expect(model.editing.last == farTask.id)
        #expect(model.list == .active)
    }

    @Test func aClosedBlockerAndAnEmptyTitleRefuseOpening() throws {
        let model = try openModel()
        model.add(title: "b", planning: .active)
        model.add(title: "a", planning: .active)
        let (a, b) = (model.tasks[1], model.tasks[0])
        model.setBlockedBy(a, to: [b.id])
        model.startEditing(a)
        model.edit(a, title: " ", description: "", priority: 4, tags: [])
        #expect(!model.open(id: b.id))
        model.edit(a, title: "a", description: "", priority: 4, tags: [])
        #expect(model.open(id: b.id))
        model.back()
        model.close(model.tasks[0], as: .rejected)
        model.commit()
        #expect(!model.open(id: b.id))
        #expect(model.editing == [a.id])
    }

    @Test func backIsRefusedWhileTheTitleOnTopIsEmptyAndShowsTheLatestSavedState() throws {
        let model = try openModel()
        model.add(title: "b", planning: .active)
        model.add(title: "a", planning: .active)
        let (a, b) = (model.tasks[1], model.tasks[0])
        model.setBlockedBy(a, to: [b.id])
        model.startEditing(a)
        model.open(id: model.tasks[1].blockedBy[0].id)
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
        model.list = .backlog
        let far = model.tasks[0]
        model.list = .active
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

    @Test func aSubtaskIsListedAfterItsParentWithTheMarkers() throws {
        let model = try openModel()
        model.add(title: "Parent", planning: .backlog)
        #expect(model.addSubtask(title: "Child", under: model.tasks[0]))
        #expect(!model.addSubtask(title: " ", under: model.tasks[0]))
        #expect(model.tasks.map(\.title) == ["Parent", "Child"])
        #expect(model.tasks.map(\.parentTitle) == [nil, "Parent"])
        #expect(model.tasks.map(\.subtasksDone) == [0, 0])
        #expect(model.tasks.map(\.subtasksTotal) == [1, 0])
    }

    @Test func aTaskThatIsParentAndSubtaskHasBothMarkersAndDoneSubtasksAreCounted() throws {
        let model = try openModel()
        model.add(title: "Parent", planning: .active)
        model.addSubtask(title: "Child", under: model.tasks[0])
        model.addSubtask(title: "Grandchild", under: model.tasks[1])
        model.toggleDone(model.tasks[2])
        model.commit()
        #expect(model.tasks.map(\.title) == ["Parent", "Child"])
        #expect(model.tasks[1].parentTitle == "Parent")
        #expect(model.tasks[1].subtasksTotal == 1 && model.tasks[1].subtasksDone == 1)
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

    @Test func aClosedSubtaskLeavesTheList() throws {
        let model = try openModel()
        model.add(title: "Parent", planning: .active)
        model.addSubtask(title: "Child", under: model.tasks[0])
        model.close(model.tasks[1], as: .rejected)
        model.commit()
        #expect(model.tasks.map(\.title) == ["Parent"])
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

    @Test func aSubtaskIsDraggedAboveItsParentAndKeepsIt() throws {
        let model = try openModel()
        model.add(title: "a", planning: .active)
        model.addSubtask(title: "b", under: model.tasks[0])
        let parent = model.tasks[0].id
        model.move(from: [1], to: 0)
        #expect(model.tasks.map(\.title) == ["b", "a"])
        #expect(model.tasks.map(\.parent) == [parent, nil])
    }

    @Test func aSubtaskIsDraggedAmongItsSiblingsAndTheNewOrderShowsInTheList() throws {
        let model = try openModel()
        model.add(title: "p", planning: .active)
        model.add(title: "other", planning: .active)
        let p = model.tasks[0]
        for title in ["x", "y", "z"] { model.addSubtask(title: title, under: p) }
        model.moveSubtask(of: p.id, from: [2], to: 0)
        #expect(model.subtasks(of: p.id).map(\.title) == ["z", "x", "y"])
        #expect(model.tasks.map(\.title) == ["p", "z", "x", "y", "other"])
        model.moveSubtask(of: p.id, from: [0], to: 3)
        #expect(model.subtasks(of: p.id).map(\.title) == ["x", "y", "z"])
        #expect(model.subtasks(of: p.id).map(\.parent) == [p.id, p.id, p.id])
        #expect(try openModel().subtasks(of: p.id).map(\.title) == ["x", "y", "z"])
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
        model.list = .active
        model.addSubtask(title: "sub", under: model.tasks[0])
        model.setBlockedBy(model.tasks[0], to: [model.tasks[2].id])
        #expect(model.blockerCandidates(for: model.tasks[0]).map(\.title) == ["later"])
        #expect(model.blockerCandidates(for: model.tasks[2]).map(\.title) == ["sub", "later"])
    }

    // MARK: Filters

    @Test func aQueryOffersKnownTagsAfterAtAndMarksUnknownOnes() throws {
        let model = try openModel()
        model.add(title: "t", tags: ["work", "network", "home"])
        #expect(model.suggestQueryTags("@a & !@w") == ["work", "network"])
        #expect(model.suggestQueryTags("@work ").isEmpty)
        #expect(TasksModel.completingQuery("@a & !@w", with: "work") == "@a & !@work")
        let check = model.checkQuery("@work | !@nope")
        #expect(check.problem == nil)
        #expect(check.unknown == ["nope"])
        #expect(TasksModel.allowedQueryText("@Work & !@Ho_me") == "@work & !@home")
    }

    @Test func aFilterKeepsNamingATagAfterItIsDeleted() throws {
        let model = try openModel()
        model.add(title: "t", tags: ["x"])
        model.addFilter(title: "F", query: "!@x")
        model.deleteTag("x")
        #expect(model.filters[0].query == "!@x")
        #expect(model.checkQuery("!@x").unknown == ["x"])
    }

    @Test func aFilterIsCreatedAtTheEndOfTheListAndShowsTheActiveTasksItsQueryMatches() throws {
        let model = try openModel()
        model.add(title: "Reply", tags: ["work"])
        model.add(title: "Review", tags: ["work", "in-review"])
        model.add(title: "Milk", tags: ["home"])
        model.add(title: "Plan", planning: .backlog, tags: ["work"])
        #expect(model.filters.isEmpty)

        #expect(model.addFilter(title: "Work", query: "@work & !@in-review"))
        #expect(model.addFilter(title: "Home", query: "@home"))
        #expect(model.filters.map(\.title) == ["Work", "Home"])

        model.list = .filter(model.filters[0].id)
        #expect(model.tasks.map(\.title) == ["Reply"])
        model.list = .filter(model.filters[1].id)
        #expect(model.tasks.map(\.title) == ["Milk"])
        model.list = .filters
        #expect(model.tasks.isEmpty)
    }

    @Test func aFilterWithoutTitleOrWithAnUnreadableQueryIsRefused() throws {
        let model = try openModel()
        #expect(!model.addFilter(title: "  ", query: "@work"))
        #expect(!model.addFilter(title: "Work", query: "@work &"))
        #expect(model.filters.isEmpty)
        #expect(model.checkQuery("@work &").problem?.position == 7)
        #expect(model.checkQuery("@work").problem == nil)
    }

    @Test func aTaskLeavesAFilterAsSoonAsItNoLongerMatches() throws {
        let model = try openModel()
        model.add(title: "Reply", tags: ["work"])
        model.addFilter(title: "Work", query: "@work")
        model.list = .filter(model.filters[0].id)
        #expect(model.tasks.map(\.title) == ["Reply"])

        model.plan(model.tasks[0], as: .backlog)
        #expect(model.tasks.isEmpty)
    }

    @Test func addingFromAFilterStaysInItAndAddsToActive() throws {
        let model = try openModel()
        model.addFilter(title: "Work", query: "@work")
        let filter = TasksList.filter(model.filters[0].id)
        model.list = filter

        model.add(title: "Unrelated")
        model.add(title: "Reply", tags: ["work"])
        #expect(model.list == filter)
        #expect(model.tasks.map(\.title) == ["Reply"])
        model.list = .active
        #expect(model.tasks.map(\.title) == ["Unrelated", "Reply"])
    }

    @Test func aTaskInAFilterIsNotMovedFromThere() throws {
        let model = try openModel()
        model.add(title: "one", tags: ["work"])
        model.add(title: "two", tags: ["work"])
        model.addFilter(title: "Work", query: "@work")
        model.list = .filter(model.filters[0].id)

        model.move(from: IndexSet(integer: 1), to: 0)
        #expect(model.tasks.map(\.title) == ["one", "two"])
    }

    @Test func aFilterIsEditedKeepingItsPlaceAndRefusingWhatCreatingRefuses() throws {
        let model = try openModel()
        model.addFilter(title: "Work", query: "@work")
        model.addFilter(title: "Home", query: "@home")
        let id = model.filters[0].id

        #expect(model.editFilter(id, title: "Office", query: "@office"))
        #expect(model.filters.map(\.title) == ["Office", "Home"])
        #expect(model.filters[0].query == "@office")
        #expect(!model.editFilter(id, title: " ", query: "@office"))
        #expect(!model.editFilter(id, title: "Office", query: "@office |"))
        #expect(model.filters[0].query == "@office")
    }

    @Test func deletingAFilterLeavesTheTasksAndLeavesItsOpenList() throws {
        let model = try openModel()
        model.add(title: "Reply", tags: ["work"])
        model.addFilter(title: "Work", query: "@work")
        model.addFilter(title: "Home", query: "@home")
        let id = model.filters[0].id
        model.list = .filter(id)

        model.deleteFilter(id)
        #expect(model.filters.map(\.title) == ["Home"])
        #expect(model.list == .filters)
        model.list = .active
        #expect(model.tasks.map(\.title) == ["Reply"])
    }

    @Test func filtersAreReorderedAndCountTheirActiveTasks() throws {
        let model = try openModel()
        model.add(title: "one", tags: ["a"])
        model.add(title: "two", tags: ["a"])
        model.addFilter(title: "A", query: "@a")
        model.addFilter(title: "B", query: "@b")
        model.addFilter(title: "C", query: "@c")
        #expect(model.filters.map(\.matches) == [2, 0, 0])

        model.moveFilters(from: IndexSet(integer: 2), to: 0)
        #expect(model.filters.map(\.title) == ["C", "A", "B"])
        model.moveFilters(from: IndexSet(integer: 0), to: 3)
        #expect(model.filters.map(\.title) == ["A", "B", "C"])
    }
}
