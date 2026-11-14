#!/usr/bin/env python3
"""Usage: migrate-tags.py <folder holding Tasks/>

Adds every valid Tag the Tasks carry, grey, to Tags/tags.json and Removes
invalid Tags from the Task files."""  # `--test` runs its tests

import json
import os
import re
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
    os.makedirs(os.path.dirname(path), exist_ok=True)
    fd, temp = tempfile.mkstemp(dir=os.path.dirname(path), suffix=".tmp")
    try:
        with os.fdopen(fd, "w", encoding="utf-8", newline="") as f:
            f.write(text)
        os.replace(temp, path)
    except BaseException:
        os.unlink(temp)
        raise


def unquote(item):
    item = item.strip()
    if len(item) >= 2 and item[0] == item[-1] == '"':
        return item[1:-1]
    return item


def clean_task(text, stamp):
    """(Tags kept, new text or None if nothing changes). Only `tags` and
    `updated` are touched; no front matter or no `tags` means no change."""
    lines = text.splitlines(keepends=True)
    if not lines or lines[0].rstrip("\r\n") != "---":
        return [], None
    close = next((i for i, l in enumerate(lines[1:], 1) if l.rstrip("\r\n") == "---"), None)
    if close is None:
        return [], None
    eol = "\r\n" if lines[0].endswith("\r\n") else "\n"
    kept, out, in_tags, removed = [], [], False, False
    for i, line in enumerate(lines):
        bare = line.rstrip("\r\n")
        if 0 < i < close:
            if bare.startswith("tags:"):
                in_tags = True
            elif in_tags and bare.lstrip().startswith("- "):
                tag = unquote(bare.lstrip()[2:])
                if is_valid(tag):
                    kept.append(tag)
                else:
                    removed = True
                    continue
            elif bare.strip():
                in_tags = False
        out.append(line)
    if not removed:
        return kept, None
    stamped = False
    for i, line in enumerate(out):
        if line.startswith("updated:") and i < close:
            out[i] = f"updated: {stamp}{eol}"
            stamped = True
            break
    if not stamped:
        at = next(i for i, l in enumerate(out[1:], 1) if l.rstrip("\r\n") == "---")
        out.insert(at, f"updated: {stamp}{eol}")
    return kept, "".join(out)


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
