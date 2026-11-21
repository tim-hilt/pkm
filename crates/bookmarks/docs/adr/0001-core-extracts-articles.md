# The core extracts Articles and writes them as Markdown

Builds on ADR-0001 and ADR-0028. Every client, the wasm web app included, has to turn a page's HTML into an Article, so `pkm-bookmarks` does it once: a hand-written HTML tokenizer, Readability-style scoring to find the main text and decide whether the page has an Article at all, and conversion to Markdown, which the Reader styles natively as Notes do. Clients only fetch the HTML and the images.

## Considered Options

- **Readability.js in a web view**: proven, but a dependency, a web view on every Save, and a second implementation for the web app's worker.
- **Keeping simplified HTML**: keeps more (merged table cells, captions), but needs an HTML renderer per client and DOM-based anchors for Markup.

## Consequences

- Tables with merged cells, figure captions and embeds are lost or flattened.
- Improving extraction never changes an Article already kept (ADR-0002).
