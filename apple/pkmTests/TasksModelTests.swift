import Foundation
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

    @Test func anEmptyTitleAddsNothing() throws {
        let model = try openModel()
        #expect(!model.add(title: "  \n", planning: .active))
        #expect(model.tasks.isEmpty)
    }

    @Test func tasksStayAcrossLaunches() throws {
        try openModel().add(title: "Kept", planning: .active)
        #expect(try openModel().tasks.map(\.title) == ["Kept"])
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

    @Test func editingToAnEmptyTitleChangesNothing() throws {
        let model = try openModel()
        model.add(title: "Keep", planning: .active)
        #expect(!model.edit(model.tasks[0], title: " ", description: "x", priority: 1, tags: []))
        #expect(model.tasks.map(\.title) == ["Keep"])
    }

    @Test func tagsAreAddedAndRemovedOnSave() throws {
        let model = try openModel()
        model.add(title: "t", planning: .active)
        #expect(model.edit(model.tasks[0], title: "t", description: "", priority: 4, tags: ["home", "work/x"]))
        #expect(model.tasks[0].tags == ["home", "work/x"])
        #expect(model.edit(model.tasks[0], title: "t", description: "", priority: 4, tags: ["work/x"]))
        #expect(try openModel().tasks[0].tags == ["work/x"])
    }

    @Test func anInvalidTagSavesNothing() throws {
        let model = try openModel()
        model.add(title: "t", planning: .active)
        #expect(!model.edit(model.tasks[0], title: "new", description: "", priority: 4, tags: ["Bad"]))
        #expect(model.tasks[0].title == "t")
    }

    @Test func knownTagsAreSuggestedByPrefix() throws {
        let model = try openModel()
        model.add(title: "t", planning: .active)
        model.edit(model.tasks[0], title: "t", description: "", priority: 4, tags: ["home", "work"])
        #expect(model.knownTags(prefix: "w") == ["work"])
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

    @Test func candidatesLeaveOutTheTaskItsParentItsSubtasksAndExistingBlockers() throws {
        let model = try openModel()
        for title in ["a", "b"] { model.add(title: title, planning: .active) }
        model.add(title: "later", planning: .backlog)
        model.planning = .active
        model.addSubtask(title: "sub", under: model.tasks[0])
        model.setBlockedBy(model.tasks[0], to: [model.tasks[2].id])
        #expect(model.blockerCandidates(for: model.tasks[0]).map(\.title) == ["later"])
        #expect(model.blockerCandidates(for: model.tasks[2]).map(\.title) == ["a", "sub", "later"])
    }
}
