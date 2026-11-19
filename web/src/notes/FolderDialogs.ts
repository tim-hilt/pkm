import { button, make } from "../Dom";
import { confirmButton, field, modal, textInput } from "../tasks/TaskDialogs";
import type { NotesModel } from "./NotesModel";

/** Why the core refused: its message. */
export function reason(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** A line under a dialog's fields that says why its action was refused. */
function refusal(): HTMLElement {
  const line = make("p", "tasks-failure");
  line.setAttribute("role", "alert");
  line.hidden = true;
  return line;
}

/** Runs `action`; if it throws, shows why in `line` and returns false. */
function attempt(line: HTMLElement, action: () => void): boolean {
  try {
    action();
    return true;
  } catch (error) {
    line.textContent = reason(error);
    line.hidden = false;
    return false;
  }
}

/** Asks for a Folder's name; `apply` throws the core's reason for one it refuses, which is shown and the dialog stays. */
export function nameDialog(
  host: HTMLElement,
  title: string,
  confirm: string,
  name: string,
  apply: (name: string) => void,
): void {
  const input = textInput(name);
  input.setAttribute("aria-label", "Name");
  const line = refusal();
  const { dialog, form } = modal(
    host,
    title,
    [field("Name", input), line],
    [button("", "Cancel", () => dialog.close()), confirmButton(confirm)],
  );
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    if (attempt(line, () => apply(input.value))) dialog.close();
  });
  input.select();
}

/** Asks where to Move to, among the Folders; `apply` throws the core's reason for a Move it refuses. */
export function moveDialog(
  host: HTMLElement,
  model: NotesModel,
  apply: (folder: string) => void,
): void {
  const line = refusal();
  const list = make(
    "ul",
    "move-list",
    model.outline.map(({ name, path, depth }) => {
      const choose = button("task-open", name, () => {
        if (attempt(line, () => apply(path))) dialog.close();
      });
      choose.style.paddingLeft = `${depth * 16 + 8}px`;
      return make("li", "task-row", [choose]);
    }),
  );
  const { dialog } = modal(
    host,
    "Move to…",
    [list, line],
    [button("", "Cancel", () => dialog.close())],
  );
}

/** Asks to Delete the Folder at `path` with the counts of what goes with it. */
export function deleteFolderDialog(host: HTMLElement, model: NotesModel, path: string): void {
  const name = path.slice(path.lastIndexOf("/") + 1);
  const confirm = confirmButton("Delete");
  const { dialog, form } = modal(
    host,
    `Delete “${name}”?`,
    [make("p", "", [model.summary(path)])],
    [button("", "Cancel", () => dialog.close()), confirm],
  );
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    model.deleteFolder(path);
    dialog.close();
  });
}
