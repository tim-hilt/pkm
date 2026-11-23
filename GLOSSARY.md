# pkm

A personal knowledge management app combining Tasks, Notes, Drawings, Bookmarks and Books, native on Apple devices (later Android and Linux), with a web app for everywhere else, storing everything locally.

## Language

### App

**Context**:
One of the app's five areas (Tasks, Notes, Drawings, Bookmarks, Books), each with its own items and its own language.
_Avoid_: Section, space, area, module, concern

**Item**:
One Task, Note, Drawing, Bookmark or Book: what a Context holds, and what a Link points to.
_Avoid_: Entry, object, document

**Item ID**:
The random identifier an Item gets when it is created and keeps for life, through renames and edits; what links and Links use.
_Avoid_: Name, hash, key

**Delete**:
Taking an Item out of its Context, with its file and everything recorded about it, on every Device; it wins over edits made elsewhere at the same time.
_Avoid_: Remove, trash

**Task**:
One thing to do, as listed in the Tasks Context.
_Avoid_: Todo

**Note**:
One written Markdown text in the Notes Context.
_Avoid_: Page, document

**Bookmark**:
One saved web page in the Bookmarks Context, with or without its Article.
_Avoid_: Link (that is a reference between items), favourite

**Book**:
One PDF or EPUB to read in the Books Context, including papers and other non-book texts.
_Avoid_: E-book, document, paper, bookmark (when meaning a saved position in a Book)

**Tag**:
A name the user puts on Items, of lowercase letters, digits, `-` and `/`. It exists from when it is first put on an Item until it is Deleted, whether or not any Item still carries it. Taking it off one Item is **Removing** it.
_Avoid_: Label, category, hashtag

**Tag Color**:
The color a Tag is shown in, wherever it appears and on every Device; grey unless the user picks another.
_Avoid_: Label color, Tag theme

**Relevance**:
How often a Tag has been put on Items, raised by one each time, on every Device; every list of Tags is ordered by it, highest first, alphabetically among equals.
_Avoid_: Score, frequency, popularity, count

**Delete a Tag**:
Ending a Tag in Settings: it is Removed from every Item, Open or Closed, and its Tag Color forgotten.
_Avoid_: Remove (that is one Item), clear

**Link**:
A reference from one item to another, possibly in another Context.
_Avoid_: Bookmark, relation

### Reading

**Reading Progress**:
Where in a Book or Article the user last left off. It moves back if the user goes back.
_Avoid_: Bookmark, position, furthest page

**Markup**:
Everything the user adds on top of a Book or an Article: Highlights and Comments. Never part of what was read.
_Avoid_: Annotation

**Highlight**:
A passage of a Book's or Article's text marked in a Highlight Color. It may run across pages or paragraphs and may have a Comment.
_Avoid_: Marker, annotation

**Comment**:
Text the user writes, belonging either to a Highlight or to a spot in a Book or Article.
_Avoid_: Note (that is a Context of its own), popup note, sticky note

**Highlight Colors**:
The colors a Highlight can have, the same in Books and Bookmarks: six built-in ones, yellow first, plus the user's Custom Colors.
_Avoid_: Palette, Page Colors

**Custom Color**:
A Highlight Color the user added. Unlike the built-in ones, it can be removed.
_Avoid_: User color

**Default Highlight Color**:
The Highlight Color every new Highlight gets. Yellow unless the user picks another.
_Avoid_: Last used color, current color

### Look

**Dark mode**:
The dark look of the app, on when the Appearance is Dark, or System while the system's appearance is dark. It only changes what is shown: never an item, never an export.
_Avoid_: Theme, dark theme, night mode

**Appearance**:
The user's setting for Dark mode: System (follow the system's appearance, the default), Light or Dark. Picked in Settings and in the Mac's View menu; it applies to the whole app at once and is kept per Device.
_Avoid_: Theme, colour scheme

### Cloud

**Device**:
One installation of the app; for the web app, one browser profile on its site (clearing the site's data removes that Device's items).
_Avoid_: Client, installation

**Cloud Backing**:
Keeping the items of every Context in the user's cloud storage as well, so that all their Devices share them.
_Avoid_: Cloud sync, backup

**Cloud Provider**:
The service that holds the items for Cloud Backing: WebDAV, iCloud Drive, Google Drive or OneDrive. Only one is Connected at a time.
_Avoid_: Backend, storage, service

**Connect**:
Starting Cloud Backing with a Cloud Provider. Its opposite is **Disconnect**, which stops it and keeps every item on the Device.
_Avoid_: Sign in, link, log out

**Reconnect**:
Giving a Connected Cloud Provider access again after it lost it, e.g. a revoked sign-in or a changed password.
_Avoid_: Sign in again, re-authenticate

**Sync**:
Exchanging changes to the items with the Cloud Provider.
_Avoid_: Backup, upload
