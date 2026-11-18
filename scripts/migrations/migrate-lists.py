#!/usr/bin/env python3
"""Usage: migrate-lists.py <folder holding Tasks/>

Turns Active and the Backlog into Filters (ADR-0025) and Deletes Closed Tasks
and the History (ADR-0026): Backlog Tasks get the Tag `backlog`; `state`,
`position` and `order` leave the Task files; `created` and `subtask-position`
join them; the Favorites All Tasks, Active and Backlog are created; other
Filters get `& !@backlog`."""  # `--test` runs its tests

import json
import os
import re
import shutil
import stat
import sys
import tempfile
import unittest
from datetime import datetime, timezone

BACKLOG = "backlog"
NOT_BACKLOG = " & !@backlog"
FAVORITES = [  # (id, title, query); fixed IDs make a rerun find them
    ("tasksall2222", "All Tasks", "*"),
    ("tasksactive2", "Active", "!@backlog"),
    ("tasksbacklog", "Backlog", "@backlog"),
]
ID = re.compile(r"[a-z2-7]{12}")


def stamp_of(seconds):
    return datetime.fromtimestamp(seconds, timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def number(x):
    """As Rust prints an f64: `1`, `2.5`."""
    return str(int(x)) if float(x).is_integer() else repr(float(x))


def to_float(text):
    try:
        return float(text)
    except (TypeError, ValueError):
        return None


# --- Files -----------------------------------------------------------------


def write_atomically(path, text):
    """Replaces `path` whole through a rename, keeping its permissions (the
    WebDAV server must still read it)."""
    folder = os.path.dirname(path)
    os.makedirs(folder, exist_ok=True)
    fd, temp = tempfile.mkstemp(dir=folder, suffix=".tmp")
    try:
        with os.fdopen(fd, "w", encoding="utf-8", newline="") as f:
            f.write(text)
            f.flush()
            os.fsync(f.fileno())
        if os.path.exists(path):
            mode = stat.S_IMODE(os.stat(path).st_mode)
        else:
            umask = os.umask(0)
            os.umask(umask)
            mode = 0o666 & ~umask
        os.chmod(temp, mode)
        os.replace(temp, path)
    except BaseException:
        os.unlink(temp)
        raise


# --- Front matter, as `pkm_base::front_matter` -----------------------------


class Doc:
    def __init__(self, entries, body):
        self.entries, self.body = entries, body  # entries: [key, str | list]

    @staticmethod
    def parse(text):
        """None for a file without front matter or a malformed one."""
        lines = text.splitlines(keepends=True)
        if not lines or lines[0].rstrip("\r\n") != "---":
            return None
        entries = []
        for at, raw in enumerate(lines[1:], 1):
            line = raw.rstrip("\r\n")
            if line == "---":
                return Doc(entries, "".join(lines[at + 1 :]))
            if not line.strip():
                continue
            if line.lstrip().startswith("- "):
                if not entries or not isinstance(entries[-1][1], list):
                    return None
                entries[-1][1].append(unquote(line.lstrip()[2:].strip()))
            else:
                key, colon, value = line.partition(":")
                if not colon or not key or key != key.strip() or " " in key:
                    return None
                value = value.strip()
                entries.append([key, unquote(value) if value else []])
        return None

    def get(self, key):
        return next((v for k, v in self.entries if k == key), None)

    def scalar(self, key):
        value = self.get(key)
        return value if isinstance(value, str) else None

    def set(self, key, value):
        for entry in self.entries:
            if entry[0] == key:
                entry[1] = value
                return
        self.entries.append([key, value])

    def remove(self, key):
        self.entries = [e for e in self.entries if e[0] != key]

    def render(self):
        out = "---\n"
        for key, value in self.entries:
            if isinstance(value, str):
                out += f"{key}: {quote(value)}\n"
            else:
                out += f"{key}:\n" + "".join(f"  - {quote(v)}\n" for v in value)
        return out + "---\n" + self.body


def is_plain(text):
    return (
        text != ""
        and text == text.strip()
        and not any(c in text for c in "\n\r\\")
        and ": " not in text
        and " #" not in text
        and not text.endswith(":")
        and text[0] not in "\"'-[]{}#&*!|>%@`?,"
    )


def quote(text):
    if is_plain(text):
        return text
    escaped = text.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n").replace("\r", "\\r")
    return f'"{escaped}"'


def unquote(text):
    if not text.startswith('"'):
        return text
    out, chars = [], iter(text[1:-1])
    for c in chars:
        if c == "\\":
            c = next(chars, "")
            c = {"n": "\n", "r": "\r"}.get(c, c)
        out.append(c)
    return "".join(out)


def is_valid_tag(tag):
    """The rules of `pkm-base`: a-z, 0-9, `-`, `/`; no leading, trailing or double `/`."""
    return re.fullmatch(r"[a-z0-9/-]+", tag) is not None and not tag.startswith("/") and not tag.endswith("/") and "//" not in tag


def tags_of(doc):
    tags = doc.get("tags")
    return [t for t in tags if is_valid_tag(t)] if isinstance(tags, list) else []


# --- Queries, as `crates/tasks/src/query.rs` -------------------------------


def parse_query(text):
    """A function of a Task's Tags, or None if `text` is no Query."""
    at = 0

    def skip():
        nonlocal at
        while at < len(text) and text[at].isspace():
            at += 1

    def chain(sep, item, join):
        nonlocal at
        parts = [item()]
        skip()
        while at < len(text) and text[at] == sep:
            at += 1
            parts.append(item())
            skip()
        return parts[0] if len(parts) == 1 else join(parts)

    def or_():
        return chain("|", and_, lambda ps: lambda tags: any(p(tags) for p in ps))

    def and_():
        return chain("&", not_, lambda ps: lambda tags: all(p(tags) for p in ps))

    def not_():
        nonlocal at
        skip()
        if at < len(text) and text[at] == "!":
            at += 1
            inner = not_()
            return lambda tags: not inner(tags)
        return atom()

    def atom():
        nonlocal at
        skip()
        if text[at : at + 1] == "*":
            at += 1
            return lambda tags: True
        if text[at : at + 1] == "@":
            start = at = at + 1
            while at < len(text) and text[at] not in "@&|!() " and not text[at].isspace():
                at += 1
            named = text[start:at]
            if not is_valid_tag(named.replace("*", "a")):
                raise ValueError(named)
            fits = re.compile(".*".join(map(re.escape, named.split("*"))))
            return lambda tags: any(fits.fullmatch(t) for t in tags)
        if text[at : at + 1] == "(":
            at += 1
            inner = or_()
            skip()
            if text[at : at + 1] != ")":
                raise ValueError("Unclosed (")
            at += 1
            return inner
        raise ValueError("Expected @tag, ! or (")

    try:
        query = or_()
        skip()
        return query if at == len(text) else None
    except (ValueError, RecursionError):
        return None


# --- The migration -----------------------------------------------------------


class Task:
    def __init__(self, doc, mtime):
        self.doc, self.mtime = doc, mtime
        self.id = doc.scalar("id")
        self.parent = doc.scalar("parent") or None
        self.position = to_float(doc.scalar("position"))
        self.order = to_float(doc.scalar("order")) or 0.0
        self.backlog = doc.scalar("state") == BACKLOG  # of its top-level Task, after `plan`


def read_history(folder):
    """When each Task was created, by Item ID; the earliest `created` line wins."""
    times = {}
    for name in sorted(os.listdir(folder)) if os.path.isdir(folder) else []:
        if name.endswith(".log"):
            with open(os.path.join(folder, name), encoding="utf-8") as f:
                for line in f:
                    parts = line.split()
                    if len(parts) == 3 and parts[1] == "created":
                        time, _, id = parts
                        times[id] = min(time, times.get(id, time))
    return times


def read_tasks(tasks):
    """The Open Task files that still have a `state`, and the paths of the Closed ones."""
    legacy, closed = [], []
    for name in sorted(os.listdir(tasks)):
        stem, ext = os.path.splitext(name)
        if ext != ".md" or not ID.fullmatch(stem):
            continue
        path = os.path.join(tasks, name)
        with open(path, encoding="utf-8", newline="") as f:
            doc = Doc.parse(f.read())
        if doc is None or doc.scalar("id") != stem:
            continue
        state = doc.scalar("state")
        if state in ("done", "rejected"):
            closed.append(path)
        elif state in ("active", BACKLOG) and doc.scalar("title") is not None:
            legacy.append(Task(doc, os.stat(path).st_mtime))
    return legacy, closed


def plan(legacy):
    """Repairs Parents as the app does (none for an orphan; a cycle broken at
    its smallest ID) and gives each Task its top-level Task's Backlog state."""
    by_id = {t.id: t for t in legacy}
    for t in legacy:
        if t.parent not in by_id:
            t.parent = None
    for start in sorted(by_id):
        seen = [start]
        while by_id[seen[-1]].parent is not None:
            parent = by_id[seen[-1]].parent
            if parent in seen:
                by_id[min(seen[seen.index(parent) :])].parent = None
                break
            seen.append(parent)
    state = {t.id: t.backlog for t in legacy}
    for t in legacy:
        root = t
        while root.parent is not None:
            root = by_id[root.parent]
        t.backlog = state[root.id]


def list_order(legacy, backlog):
    """The Active or the Backlog list, in its order. A Task without `position`
    gets one in the tree order of `order`, after the Tasks that have one."""
    tasks = [t for t in legacy if t.backlog == backlog]
    last = max((t.position for t in tasks if t.position is not None), default=0.0)
    children = {}
    for t in tasks:
        children.setdefault(t.parent, []).append(t)
    for siblings in children.values():
        siblings.sort(key=lambda t: (t.position if t.position is not None else t.order, t.id))
    place, todo = {}, list(reversed(children.pop(None, [])))
    while todo:
        t = todo.pop()
        if t.position is None:
            last += 1
        place[t.id] = t.position if t.position is not None else last
        todo.extend(reversed(children.pop(t.id, [])))
    return sorted(tasks, key=lambda t: (place[t.id], t.id))


def numbered(ids):
    """Places 1, 2, 3 ... in the order of `ids`, written in Task ID order as the app does."""
    return [f"{id} {n}" for id, n in sorted((id, n) for n, id in enumerate(ids, 1))]


def read_filters(folder):
    """(path, Doc) of each readable Filter, in the Filters list's order."""
    found = []
    for name in sorted(os.listdir(folder)) if os.path.isdir(folder) else []:
        path = os.path.join(folder, name)
        with open(path, encoding="utf-8", newline="") as f:
            doc = Doc.parse(f.read()) if name.endswith(".md") else None
        if doc and all(doc.scalar(k) is not None for k in ("id", "title", "query")) and name == doc.scalar("id") + ".md":
            found.append((to_float(doc.scalar("position")) or 0.0, doc.scalar("id"), path, doc))
    return [(path, doc) for _, _, path, doc in sorted(found, key=lambda f: (f[0], f[1]))]


def migrate_filter(doc, active, stamp):
    """Keeps an existing Filter showing what it showed, in its order; True if changed."""
    query = doc.scalar("query")
    matches = parse_query(query)
    if matches is None or (query.startswith("(") and query.endswith(")" + NOT_BACKLOG)):
        return False
    places = {}
    for entry in doc.get("places") if isinstance(doc.get("places"), list) else []:
        task, _, position = entry.partition(" ")
        if to_float(position) is not None:
            places[task] = to_float(position)
    shown = [t.id for t in active if matches(tags_of(t.doc))]
    shown = sorted((i for i in shown if i in places), key=lambda i: places[i]) + [i for i in shown if i not in places]
    doc.set("query", f"({query}){NOT_BACKLOG}")
    if shown:
        doc.set("places", numbered(shown))
    else:
        doc.remove("places")
    doc.set("updated", stamp)
    return True


def migrate(root, stamp=None, history=None):
    """Returns (Tasks rewritten, Closed Tasks Deleted, Filters written)."""
    stamp = stamp or stamp_of(datetime.now(timezone.utc).timestamp())
    tasks, filters = os.path.join(root, "Tasks"), os.path.join(root, "Filters")
    history_folder = os.path.join(tasks, ".history")
    created = read_history(history_folder) if history is None else history
    legacy, closed = read_tasks(tasks)
    plan(legacy)
    active, backlog = list_order(legacy, False), list_order(legacy, True)

    written, stored = 0, read_filters(filters)
    fixed = [f[0] for f in FAVORITES]
    for path, doc in stored:
        if doc.scalar("id") not in fixed and migrate_filter(doc, active, stamp):
            write_atomically(path, doc.render())
            written += 1
    position = max((to_float(d.scalar("position")) or 0.0 for _, d in stored), default=0.0)
    for (id, title, query), order in zip(FAVORITES, [active + backlog, active, backlog]):
        path = os.path.join(filters, id + ".md")
        if os.path.exists(path):
            continue
        position += 1
        doc = Doc([["id", id], ["title", title], ["query", query], ["favorite", "true"]], "")
        doc.set("position", number(position))
        if order:
            doc.set("places", numbered([t.id for t in order]))
        doc.set("updated", stamp)
        write_atomically(path, doc.render())
        written += 1

    counts = {}
    for t in active + backlog:
        if t.parent is not None:
            counts[t.parent] = counts.get(t.parent, 0) + 1
            t.doc.set("subtask-position", str(counts[t.parent]))
        for key in ("state", "position", "order"):
            t.doc.remove(key)
        if t.doc.scalar("created") is None:
            t.doc.set("created", created.get(t.id) or stamp_of(t.mtime))
        if t.backlog and BACKLOG not in tags_of(t.doc):
            t.doc.set("tags", tags_of(t.doc) + [BACKLOG])
        t.doc.set("updated", stamp)
        write_atomically(os.path.join(tasks, t.id + ".md"), t.doc.render())
    if any(t.backlog for t in legacy):
        remember_tag(root, BACKLOG, stamp)

    for path in closed:
        os.remove(path)
    shutil.rmtree(history_folder, ignore_errors=True)
    return len(legacy), len(closed), written


def remember_tag(root, tag, stamp):
    """Adds `tag`, grey, to `Tags/tags.json`, unless it is there."""
    path = os.path.join(root, "Tags", "tags.json")
    known = {}
    if os.path.exists(path):
        with open(path, encoding="utf-8") as f:
            known = json.load(f)["tags"]  # unreadable: stop rather than overwrite
    if tag not in known:
        known[tag] = {"updated": stamp}
        write_atomically(path, json.dumps({"tags": dict(sorted(known.items()))}, indent=2, ensure_ascii=False) + "\n")


# --- Tests -----------------------------------------------------------------

UPDATED = "2000-01-01T00:00:00Z"


class MigrateTests(unittest.TestCase):
    def setUp(self):
        self._dir = tempfile.TemporaryDirectory()
        self.addCleanup(self._dir.cleanup)
        self.root = self._dir.name
        os.makedirs(os.path.join(self.root, "Tasks"))

    def path(self, *parts):
        return os.path.join(self.root, *parts)

    def put(self, path, text):
        os.makedirs(os.path.dirname(self.path(path)), exist_ok=True)
        with open(self.path(path), "w", encoding="utf-8") as f:
            f.write(text)

    def task(self, id, state, extra=""):
        self.put(f"Tasks/{id}.md", f"---\nid: {id}\ntitle: {id}\npriority: 4\nstate: {state}\n{extra}updated: {UPDATED}\n---\nBody\n")

    def filter(self, id, query, places=()):
        items = "".join(f"  - {p}\n" for p in places)
        self.put(f"Filters/{id}.md", f"---\nid: {id}\ntitle: {id}\nquery: {query}\nfavorite: false\nposition: 1\n{'places:' + chr(10) + items if items else ''}updated: {UPDATED}\n---\n")

    def read(self, path):
        with open(self.path(path), encoding="utf-8") as f:
            return f.read()

    def doc(self, path):
        return Doc.parse(self.read(path))

    def shown(self, filter_id):
        """The Tasks a Filter shows, as the app orders a Manual Filter, by the first letter of their IDs."""
        filter = self.doc(f"Filters/{filter_id}.md")
        matches = parse_query(filter.scalar("query"))
        places = {e.split(" ")[0]: float(e.split(" ")[1]) for e in filter.get("places") or []}
        ids = [n[:-3] for n in os.listdir(self.path("Tasks")) if n.endswith(".md")]
        found = [i for i in ids if matches(tags_of(self.doc(f"Tasks/{i}.md")))]
        return "".join(i[0] for i in sorted(found, key=lambda i: (places.get(i, float("inf")), i)))

    def legacy_set(self):
        self.task("aaaaaaaaaaaa", "active", "position: 2\ntags:\n  - work\n")
        self.task("bbbbbbbbbbbb", "active", "position: 1\ntags:\n  - work\n")
        self.task("cccccccccccc", "backlog", "position: 1\ntags:\n  - work\n")
        self.task("dddddddddddd", "done", "position: 1\n")
        self.task("eeeeeeeeeeee", "rejected", "order: 1\n")

    def test_tasks_are_rewritten_and_closed_ones_deleted(self):
        self.legacy_set()
        self.assertEqual(migrate(self.root, "T1", {}), (3, 2, 3))
        backlog, active = self.doc("Tasks/cccccccccccc.md"), self.doc("Tasks/aaaaaaaaaaaa.md")
        self.assertEqual(backlog.get("tags"), ["work", "backlog"])
        self.assertEqual(active.get("tags"), ["work"])
        for task in (backlog, active):
            self.assertEqual([task.scalar(k) for k in ("state", "position", "order")], [None] * 3)
            self.assertEqual(task.scalar("updated"), "T1")
            self.assertIsNotNone(task.scalar("created"))
            self.assertEqual(task.body, "Body\n")
        self.assertEqual(sorted(os.listdir(self.path("Tasks"))), ["aaaaaaaaaaaa.md", "bbbbbbbbbbbb.md", "cccccccccccc.md"])
        with open(self.path("Tags", "tags.json")) as f:
            self.assertEqual(json.load(f), {"tags": {"backlog": {"updated": "T1"}}})

    def test_a_subtask_goes_to_the_backlog_with_its_top_level_task(self):
        self.task("aaaaaaaaaaaa", "backlog")
        self.task("bbbbbbbbbbbb", "active", "parent: aaaaaaaaaaaa\n")
        self.task("cccccccccccc", "active", "parent: bbbbbbbbbbbb\n")
        migrate(self.root, "T1", {})
        self.assertEqual(self.shown("tasksbacklog"), "abc")

    def test_the_three_favorites_keep_the_old_orders(self):
        self.legacy_set()
        migrate(self.root, "T1", {})
        favorites = [self.doc(f"Filters/{id}.md") for id, _, _ in FAVORITES]
        self.assertEqual(
            [(d.scalar("title"), d.scalar("query"), d.scalar("favorite"), d.scalar("position")) for d in favorites],
            [("All Tasks", "*", "true", "1"), ("Active", "!@backlog", "true", "2"), ("Backlog", "@backlog", "true", "3")],
        )
        self.assertEqual([self.shown(id) for id, _, _ in FAVORITES], ["bac", "ba", "c"])

    def test_files_with_only_an_order_list_in_the_tree_order(self):
        self.task("aaaaaaaaaaaa", "active", "order: 2\n")
        self.task("bbbbbbbbbbbb", "active", "order: 1\n")
        self.task("cccccccccccc", "active", "order: 1\n")  # ties with b: by Item ID
        self.task("dddddddddddd", "active", "parent: bbbbbbbbbbbb\norder: 2\n")
        self.task("eeeeeeeeeeee", "active", "parent: bbbbbbbbbbbb\norder: 1\n")
        self.task("ffffffffffff", "active", "parent: eeeeeeeeeeee\norder: 1\n")
        self.task("gggggggggggg", "active", "position: 9\n")
        migrate(self.root, "T1", {})
        self.assertEqual(self.shown("tasksactive2"), "gbefdca")

    def test_subtasks_are_numbered_from_their_siblings_old_order(self):
        self.task("aaaaaaaaaaaa", "backlog")
        self.task("bbbbbbbbbbbb", "active", "parent: aaaaaaaaaaaa\nposition: 9\n")
        self.task("cccccccccccc", "active", "parent: aaaaaaaaaaaa\nposition: 3\n")
        self.task("dddddddddddd", "active", "parent: aaaaaaaaaaaa\norder: 1\n")
        migrate(self.root, "T1", {})
        key = lambda id: self.doc(f"Tasks/{id}.md").scalar("subtask-position")
        self.assertEqual([key(i * 12) for i in "acbd"], [None, "1", "2", "3"])

    def test_existing_filters_show_the_same_tasks_in_the_same_order(self):
        self.legacy_set()
        self.task("ffffffffffff", "active", "position: 3\ntags:\n  - work\n")
        # Placed: f, then a; b has no place and so follows. The Backlog Task c isn't shown.
        self.filter("wwwwwwwwwwww", "@work", ["ffffffffffff 1", "aaaaaaaaaaaa 2"])
        migrate(self.root, "T1", {})
        self.assertEqual(self.shown("wwwwwwwwwwww"), "fab")
        filter = self.doc("Filters/wwwwwwwwwwww.md")
        self.assertEqual((filter.scalar("query"), filter.scalar("updated")), ("(@work) & !@backlog", "T1"))
        self.assertEqual(self.doc("Filters/tasksall2222.md").scalar("position"), "2")

    def test_a_filters_unplaced_tasks_keep_actives_order(self):
        self.task("aaaaaaaaaaaa", "active", "position: 2\ntags:\n  - work/x\n")
        self.task("bbbbbbbbbbbb", "active", "position: 1\ntags:\n  - work/y\n")
        self.filter("wwwwwwwwwwww", "@work/* & !(@home | @x)")
        migrate(self.root, "T1", {})
        self.assertEqual(self.shown("wwwwwwwwwwww"), "ba")

    def test_a_filter_that_cannot_be_read_is_left_alone(self):
        self.legacy_set()
        self.filter("wwwwwwwwwwww", "@work &")
        migrate(self.root, "T1", {})
        self.assertEqual(self.doc("Filters/wwwwwwwwwwww.md").scalar("query"), "@work &")

    def test_created_comes_from_the_history_which_is_then_deleted(self):
        self.task("aaaaaaaaaaaa", "active")
        self.task("bbbbbbbbbbbb", "active")
        os.utime(self.path("Tasks", "bbbbbbbbbbbb.md"), (1_700_000_000, 1_700_000_000))
        self.put("Tasks/.history/p-2023-11.log", "2023-11-15T00:00:00Z created aaaaaaaaaaaa\n2023-11-15T01:00:00Z moved aaaaaaaaaaaa\n")
        self.put("Tasks/.history/q-2023-11.log", "2023-11-14T22:13:20Z created aaaaaaaaaaaa\n")
        migrate(self.root, "T1")
        self.assertEqual(self.doc("Tasks/aaaaaaaaaaaa.md").scalar("created"), "2023-11-14T22:13:20Z")
        self.assertEqual(self.doc("Tasks/bbbbbbbbbbbb.md").scalar("created"), "2023-11-14T22:13:20Z")  # modified then
        self.assertFalse(os.path.exists(self.path("Tasks", ".history")))

    def test_quoted_values_survive(self):
        self.task("aaaaaaaaaaaa", "active", 'description: "a: b \\"c\\""\n')
        migrate(self.root, "T1", {})
        self.assertEqual(self.doc("Tasks/aaaaaaaaaaaa.md").scalar("description"), 'a: b "c"')

    def test_rerun_changes_nothing(self):
        self.legacy_set()
        self.filter("wwwwwwwwwwww", "@work")
        migrate(self.root, "T1", {})
        snapshot = lambda: {(d, n): self.read(f"{d}/{n}") for d in ("Tasks", "Filters", "Tags") for n in os.listdir(self.path(d))}
        before = snapshot()
        self.assertEqual(migrate(self.root, "T2", {}), (0, 0, 0))
        self.assertEqual(snapshot(), before)

    def test_keeps_permissions_and_leaves_no_temporary_files(self):
        self.task("aaaaaaaaaaaa", "backlog")
        os.chmod(self.path("Tasks", "aaaaaaaaaaaa.md"), 0o644)
        migrate(self.root, "T1", {})
        self.assertEqual(os.stat(self.path("Tasks", "aaaaaaaaaaaa.md")).st_mode & 0o777, 0o644)
        for folder in ("Tasks", "Filters", "Tags"):
            self.assertFalse([n for n in os.listdir(self.path(folder)) if n.endswith(".tmp")])


def main(argv):
    if argv == ["--test"]:
        unittest.main(argv=[sys.argv[0]])
    if len(argv) != 1 or not os.path.isdir(os.path.join(argv[0], "Tasks")):
        print(__doc__, file=sys.stderr)
        return 2
    tasks, closed, filters = migrate(argv[0])
    print(f"Rewrote {tasks} Tasks; Deleted {closed} Closed Tasks; wrote {filters} Filters.")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
