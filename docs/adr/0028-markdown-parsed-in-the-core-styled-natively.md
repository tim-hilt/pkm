# Markdown is parsed in the core, styled by each client's own text view

Descriptions (and later Notes) are edited in Live Preview: every line shows rendered Markdown except the line with the caret. `pkm-base` parses a small Markdown subset in a hand-written parser with no dependencies. It returns styled ranges, marker ranges included, and the edits for continuing a list. Each client applies those ranges to its own text view: `UITextView`/`NSTextView` (TextKit 2) on Apple, `contenteditable` on the web. The file always keeps the text exactly as typed.

Subset: headings `#`–`###`, Inline formatting, `- ` and `N. ` lists, links, images, fenced code blocks, quotes. Anything else stays literal text.

## Considered Options

- **`AttributedString(markdown:)` on Apple, a JS library on the web**: two dialects, a dependency, and Apple's parser drops the markers that Live Preview has to show.
- **Full CommonMark + GFM**: a far larger parser and editor for syntax nobody asked for.
