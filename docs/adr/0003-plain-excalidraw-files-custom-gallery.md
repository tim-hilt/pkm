# Drawings are plain `.excalidraw` files behind a custom Gallery

Each Drawing is stored as an Excalidraw-compatible `.excalidraw` JSON file in the app's Documents folder (visible in Files/Finder), autosaved, with a custom Gallery instead of `DocumentGroup`. Plain files keep full interoperability with excalidraw.com and make later sync (WebDAV, Google Drive, OneDrive, iCloud Drive) a file-sync problem rather than a database-sync problem. Element types and properties the app does not understand are preserved verbatim on save so no user data is ever lost.
