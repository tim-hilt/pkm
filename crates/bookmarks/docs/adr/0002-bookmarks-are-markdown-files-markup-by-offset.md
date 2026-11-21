# Bookmarks are Markdown files; Markup anchors by offset into an Article that never changes

Builds on ADR-0019, ADR-0029, ADR-0030 and Books ADR-0006.

Each Bookmark is `Bookmarks/<id>.md`: its details (Address, Title, Site, Cover, Excerpt, Author, Published, Saved) in front matter, its Article, if any, as the body, its images beside it as `<random-id>.<ext>`. Markup and Reading Progress live in a state file beside it, merged by Sync as Books' are. A position is a character offset into the Article's text, with the quoted text kept as a fallback. This holds because an Article is never edited nor fetched again: it can only be added to a Bookmark without one, or dropped with its Markup.

## Considered Options

- **Markup inside the `.md`**: one file per Bookmark, but every Highlight rewrites the Article's file and Sync would merge text with Markup.
- **Refreshing an Article when the page changes**: current text, but every Highlight would need re-anchoring.
