# Book details are kept by the core, on each Device only

A Book's details (stored title, author, page count) are read from the File by the client's `PdfReader` (ADR-0010), or by the core for an EPUB (ADR-0005), and kept by the core in a hidden `.details` file in the Book's folder; the core derives the Title and percentage read from them. They are never synced: every Device reads them from the same File, and Delete takes them along with the folder.

## Considered Options

- **Kept by each client** (the former `Details.json`): every client repeats the store and the rules derived from it.
- **Synced with the Book's state**: saves each new Device one read per File, but enlarges what is synced, and clients with different PDF engines could overwrite each other's reading.
