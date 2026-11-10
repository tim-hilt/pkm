# Books

The Context for PDFs and EPUBs to read and mark up. App-wide terms (Context, Item, Item ID, Delete, Appearance, Device, Cloud Backing, Sync) are in the [App glossary](../../GLOSSARY.md).

## Language

**Book**:
A PDF or EPUB the user has brought into the app to read, including papers and other non-book texts.
_Avoid_: Document, PDF, EPUB, e-book

**Format**:
Which kind of File a Book is: PDF or EPUB.
_Avoid_: Filetype, kind

**File**:
The stored bytes of a Book, as kept on disk and in cloud storage.
_Avoid_: Book (when meaning the bytes)

**Title**:
The name shown for a Book: the title stored in the PDF, or the file name if the PDF has none.
_Avoid_: Name, filename

**Author**:
The author stored in the PDF, if any.
_Avoid_: Creator

**Library**:
The set of Books the user has imported.
_Avoid_: Collection, Gallery (when meaning the set)

**Import**:
Copying a File into the Books Context, which adds a Book to the Library.
_Avoid_: Add, upload, open

**Delete**:
Taking a Book out of the Library, along with its File and everything recorded about it.
_Avoid_: Remove

**Gallery**:
The view showing the Library.
_Avoid_: File gallery, home

**Cover**:
The image shown for a Book in the Gallery: a PDF's first page, or an EPUB's cover image.
_Avoid_: Thumbnail, preview, first page

**Library Search**:
Finding Books in the Gallery by Title, file name or Author.
_Avoid_: Search (unqualified), filter

**Book Search**:
Finding text inside a single Book.
_Avoid_: Search (unqualified), find

**Book Text**:
A Book's text as the Book Search searches it: its chunks of text — a PDF's pages, an EPUB's spine items — end to end.
_Avoid_: Searched text, full text

**Table of Contents**:
The Chapters and Sections stored in a Book, in reading order.
_Avoid_: TOC, outline, bookmarks

**Chapter**:
A top-level entry in the Table of Contents, pointing to a place in the Book.
_Avoid_: Bookmark, outline item

**Section**:
An entry in the Table of Contents nested under a Chapter or another Section, at any depth.
_Avoid_: Subchapter, subsection

**Reader**:
The view in which a Book is read, one at a time.
_Avoid_: Reading mode, viewer

**Jump**:
Moving to another place in a Book by choosing a Chapter, Section, Book Search match or link, rather than by scrolling.
_Avoid_: Navigate, go to

**Back**:
Returning to where the reader was before the latest Jump.
_Avoid_: Undo, history, Go to Gallery

**Go to Gallery**:
Leaving the Reader for the Gallery.
_Avoid_: Back, close

**Immersive Mode**:
A state of the Reader in which everything except the Book's pages is hidden.
_Avoid_: Reading mode, focus mode, full screen

**Reading Progress**:
Where in a Book the user last left off. It moves back if the user goes back.
_Avoid_: Bookmark, position, furthest page

**Last Read**:
When the Reader last showed a Book.
_Avoid_: Last opened, recently opened

**Page Style**:
How a Book's pages are shown: Light Pages, Dark Pages, or Match Appearance, which follows the app's Appearance.
_Avoid_: Page theme, page color, page mode

**Light Pages**:
Showing a Book in its original colors.
_Avoid_: Normal, original mode

**Dark Pages**:
Showing a Book dark, in Page Colors.
_Avoid_: Invert, night mode

**Page Colors**:
The paper and ink colors the user picks for Dark Pages.
_Avoid_: Palette, page theme

**Markup**:
Everything the user adds on top of a Book: Highlights and Comments. Never part of the File.
_Avoid_: Annotation

**Highlight**:
A passage of a Book's text marked in a Highlight Color. It may run across pages and may have a Comment.
_Avoid_: Marker, annotation

**Comment**:
Text the user writes, belonging either to a Highlight or to a spot in the Book.
_Avoid_: Note (that is a Context of its own), popup note, sticky note

**Highlight Colors**:
The colors a Highlight can have: six built-in ones, yellow first, plus the user's Custom Colors.
_Avoid_: Palette, Page Colors

**Custom Color**:
A Highlight Color the user added. Unlike the built-in ones, it can be removed.
_Avoid_: User color

**Default Highlight Color**:
The Highlight Color every new Highlight gets. Yellow unless the user picks another.
_Avoid_: Last used color, current color
