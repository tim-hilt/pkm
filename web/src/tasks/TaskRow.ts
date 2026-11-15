import { make } from "../Dom";
import { tagLabel } from "./TagLabel";
import type { ListedTask, TasksModel } from "./TasksModel";

const subtasksGlyph = `<svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="m4 7 2 2 3-3M4 17l2 2 3-3M13 8h7M13 18h7"/></svg>`;
const parentGlyph = `<svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M5 5v8a2 2 0 0 0 2 2h11m-4-4 4 4-4 4"/></svg>`;
const lockGlyph = `<svg viewBox="0 0 24 24" width="16" height="16" fill="currentColor" aria-hidden="true"><path d="M7 10V8a5 5 0 0 1 10 0v2h1a1 1 0 0 1 1 1v9a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1v-9a1 1 0 0 1 1-1zm2 0h6V8a3 3 0 0 0-6 0z"/></svg>`;

/**
 * The line of a Task as the list and the Subtasks section draw it: the circle
 * (Done, or Reverts the Done), the Title, Description, markers and Tags, and
 * the Blocked lock. `open` is the button of the text, which `onOpen` follows;
 * `showsParent` is off where the Parent is obvious.
 */
export function taskLine(
  task: ListedTask,
  model: TasksModel,
  onOpen: () => void,
  { showsParent = true } = {},
): { line: HTMLElement; open: HTMLButtonElement } {
  const circle = make("button", `task-circle priority-${task.priority}`);
  circle.type = "button";
  circle.classList.toggle("done", task.done);
  circle.setAttribute("aria-label", task.done ? "Revert Done" : "Done");
  circle.addEventListener("click", () => model.toggleDone(task));
  const text = make("span", "task-text", [make("span", "task-title", [task.title])]);
  if (task.description) text.append(make("span", "task-description", [task.description]));
  const marks = markers(task, showsParent);
  if (marks.length > 0 || task.tags.length > 0) {
    const labels = task.tags.map((tag) => tagLabel(tag, task.tagColors[tag]));
    // Separate nodes of one line, by name, without chips.
    const tags = make("span", "task-tags", labels.flatMap((label) => [label, " "]).slice(0, -1));
    text.append(make("span", "task-meta", [...marks, tags]));
  }
  const open = make("button", "task-open", [text]);
  open.type = "button";
  open.addEventListener("click", onOpen);
  const line = make("div", "task-line", [circle, open]);
  if (task.blocked) {
    const lock = make("span", "task-blocked");
    lock.innerHTML = lockGlyph;
    lock.setAttribute("role", "img");
    lock.setAttribute("aria-label", "Blocked");
    line.append(lock);
  }
  return { line, open };
}

/** The Parent marker ("Done/total") and, if `showsParent`, the Subtask marker (its Parent's Title). */
function markers(task: ListedTask, showsParent: boolean): HTMLElement[] {
  const marks: HTMLElement[] = [];
  if (task.subtasksTotal > 0) {
    const mark = make("span", "task-mark", [`${task.subtasksDone}/${task.subtasksTotal}`]);
    mark.insertAdjacentHTML("afterbegin", subtasksGlyph);
    mark.title = `${task.subtasksDone} of ${task.subtasksTotal} Subtasks Done`;
    marks.push(mark);
  }
  if (showsParent && task.parentTitle !== undefined) {
    // The Title in its own span, which alone can end in an ellipsis.
    const title = make("span", "task-parent-title", [task.parentTitle]);
    const mark = make("span", "task-mark task-parent", [title]);
    mark.insertAdjacentHTML("afterbegin", parentGlyph);
    mark.title = `Subtask of ${task.parentTitle}`;
    marks.push(mark);
  }
  return marks;
}
