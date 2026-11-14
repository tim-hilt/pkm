#!/usr/bin/env python3
"""Usage: migrate-tags.py <folder holding Tasks/>

Adds every valid Tag the Tasks carry, grey, to Tags/tags.json and Removes
invalid Tags from the Task files."""  # `--test` runs its tests

import json
import os
import re
import stat
import sys
import tempfile
import unittest
from datetime import datetime, timezone

TAGS_FILE = os.path.join("Tags", "tags.json")


def is_valid(tag):
    """The rules of `pkm-base`: a-z, 0-9, `-`, `/`; no leading, trailing or double `/`."""
    return (
        re.fullmatch(r"[a-z0-9/-]+", tag) is not None
        and not tag.startswith("/")
        and not tag.endswith("/")
        and "//" not in tag
    )


def now():
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def write_atomically(path, text):
    """Replaces `path` whole through a rename, so a crash leaves the old or
    the new file. It keeps the old file's permissions, as the WebDAV server
    must still read it; `mkstemp` alone would make it private."""
    folder = os.path.dirname(path)
    os.makedirs(folder, exist_ok=True)
    fd, temp = tempfile.mkstemp(dir=folder, suffix=".tmp")
    try:
        with os.fdopen(fd, "w", encoding="utf-8", newline="") as f:
            f.write(text)
            f.flush()
            os.fsync(f.fileno())
        os.chmod(temp, stat.S_IMODE(os.stat(path).st_mode) if os.path.exists(path) else new_file_mode())
        os.replace(temp, path)
    except BaseException:
        os.unlink(temp)
        raise


def new_file_mode():
    """The permissions a file created with `open` gets."""
    umask = os.umask(0)
    os.umask(umask)
    return 0o666 & ~umask


def unquote(item):
    item = item.strip()
    if len(item) >= 2 and item[0] == item[-1] == '"':
        return item[1:-1]
    return item


def clean_task(text, stamp):
    """(Tags kept, new text or None if nothing changes). Only `tags` and
    `updated` are touched, as the app writes them: an emptied `tags` is
    left out. No front matter or no `tags` means no change."""
    lines = text.splitlines(keepends=True)
    bare = [line.rstrip("\r\n") for line in lines]
    if not bare or bare[0] != "---" or "---" not in bare[1:]:
        return [], None
    close = bare.index("---", 1)
    kept, dropped, in_tags, tags_at = [], set(), False, None
    for i in range(1, close):
        if bare[i].startswith("tags:"):
            in_tags, tags_at = True, i
        elif in_tags and bare[i].lstrip().startswith("- "):
            tag = unquote(bare[i].lstrip()[2:])
            if is_valid(tag):
                kept.append(tag)
            else:
                dropped.add(i)
        elif bare[i].strip():
            in_tags = False
    if not dropped:
        return kept, None
    if not kept:
        dropped.add(tags_at)
    front = [line for i, line in enumerate(lines[:close]) if i not in dropped]
    stamped = f"updated: {stamp}{lines[0][3:]}"  # with the file's line ending
    at = next((i for i, line in enumerate(front) if line.startswith("updated:")), None)
    if at is None:
        front.append(stamped)
    else:
        front[at] = stamped
    return kept, "".join(front + lines[close:])


def migrate(root, stamp=None):
    """Returns (Tags added, Task files cleaned)."""
    stamp = stamp or now()
    tags_path = os.path.join(root, TAGS_FILE)
    known = {}
    if os.path.exists(tags_path):
        with open(tags_path, encoding="utf-8") as f:
            known = json.load(f)["tags"]  # unreadable: stop rather than overwrite
    found, cleaned = set(), 0
    tasks = os.path.join(root, "Tasks")
    for name in sorted(os.listdir(tasks)):
        if not name.endswith(".md"):
            continue
        path = os.path.join(tasks, name)
        with open(path, encoding="utf-8", newline="") as f:
            kept, text = clean_task(f.read(), stamp)
        found.update(kept)
        if text is not None:
            write_atomically(path, text)
            cleaned += 1
    added = sorted(found - known.keys())
    if added:
        for tag in added:
            known[tag] = {"updated": stamp}
        body = {"tags": dict(sorted(known.items()))}
        write_atomically(tags_path, json.dumps(body, indent=2, ensure_ascii=False) + "\n")
    return len(added), cleaned


# --- Tests -----------------------------------------------------------------


def task(tags, updated="2024-01-01T00:00:00Z"):
    items = "".join(f"  - {t}\n" for t in tags)
    return f"---\nid: aaaaaaaaaaaa\ntitle: Milk\ntags:\n{items}updated: {updated}\n---\nBody\n"


class MigrateTests(unittest.TestCase):
    def setUp(self):
        self._dir = tempfile.TemporaryDirectory()
        self.addCleanup(self._dir.cleanup)
        self.root = self._dir.name
        os.makedirs(os.path.join(self.root, "Tasks"))

    def put(self, name, text):
        with open(os.path.join(self.root, "Tasks", name), "w", encoding="utf-8") as f:
            f.write(text)

    def read(self, *parts):
        with open(os.path.join(self.root, *parts), encoding="utf-8") as f:
            return f.read()

    def tags(self):
        return json.loads(self.read(TAGS_FILE))["tags"]

    def test_adds_valid_tags_grey(self):
        self.put("a.md", task(["home", "work/x"]))
        self.put("b.md", task(["home"]))
        self.assertEqual(migrate(self.root, "T1"), (2, 0))
        self.assertEqual(self.tags(), {"home": {"updated": "T1"}, "work/x": {"updated": "T1"}})

    def test_keeps_existing_tags_and_colors(self):
        os.makedirs(os.path.join(self.root, "Tags"))
        with open(os.path.join(self.root, TAGS_FILE), "w") as f:
            json.dump({"tags": {"home": {"color": "#d0021b", "updated": "T0"}, "old": {"updated": "T0"}}}, f)
        self.put("a.md", task(["home", "new"]))
        self.assertEqual(migrate(self.root, "T1"), (1, 0))
        self.assertEqual(
            self.tags(),
            {
                "home": {"color": "#d0021b", "updated": "T0"},
                "new": {"updated": "T1"},
                "old": {"updated": "T0"},
            },
        )

    def test_removes_invalid_tags_changing_only_tags_and_updated(self):
        self.put("a.md", task(["home", "Bad", "a//b", "/x", "ok-1"]))
        self.assertEqual(migrate(self.root, "T1"), (2, 1))
        self.assertEqual(
            self.read("Tasks", "a.md"),
            task(["home", "ok-1"], updated="T1"),
        )
        self.assertEqual(sorted(self.tags()), ["home", "ok-1"])

    def test_drops_the_tags_property_once_no_tag_is_left(self):
        self.put("a.md", task(["Bad"]))
        migrate(self.root, "T1")
        self.assertEqual(self.read("Tasks", "a.md"), task([], updated="T1").replace("tags:\n", ""))

    def test_adds_updated_to_the_front_matter_not_the_body(self):
        self.put("a.md", "---\ntags:\n  - Bad\n  - B2\n---\nupdated: never\n")
        migrate(self.root, "T1")
        self.assertEqual(self.read("Tasks", "a.md"), "---\nupdated: T1\n---\nupdated: never\n")

    def test_keeps_the_permissions_of_a_rewritten_file(self):
        self.put("a.md", task(["Bad", "home"]))
        path = os.path.join(self.root, "Tasks", "a.md")
        os.chmod(path, 0o644)
        migrate(self.root, "T1")
        self.assertEqual(os.stat(path).st_mode & 0o777, 0o644)

    def test_rerun_changes_nothing(self):
        self.put("a.md", task(["home", "Bad"]))
        migrate(self.root, "T1")
        before = (self.read("Tasks", "a.md"), self.read(TAGS_FILE))
        self.assertEqual(migrate(self.root, "T2"), (0, 0))
        self.assertEqual((self.read("Tasks", "a.md"), self.read(TAGS_FILE)), before)

    def test_files_are_untouched_when_nothing_changes(self):
        self.put("a.md", task(["home"]))
        migrate(self.root, "T1")
        paths = [os.path.join(self.root, "Tasks", "a.md"), os.path.join(self.root, TAGS_FILE)]
        for p in paths:
            os.utime(p, (1, 1))
        migrate(self.root, "T2")
        self.assertEqual([os.stat(p).st_mtime for p in paths], [1, 1])

    def test_no_temporary_files_are_left(self):
        self.put("a.md", task(["Bad", "home"]))
        migrate(self.root)
        for folder in ("Tasks", "Tags"):
            self.assertFalse([n for n in os.listdir(os.path.join(self.root, folder)) if n.endswith(".tmp")])


def main(argv):
    if argv == ["--test"]:
        unittest.main(argv=[sys.argv[0]])
    if len(argv) != 1 or not os.path.isdir(os.path.join(argv[0], "Tasks")):
        print(__doc__, file=sys.stderr)
        return 2
    added, cleaned = migrate(argv[0])
    print(f"Added {added} Tags; cleaned {cleaned} Task files.")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
