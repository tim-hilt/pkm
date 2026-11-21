# The Share extension Saves into an App Group inbox, which the app takes in

Builds on ADR-0001 and ADR-0002. The extension is a separate app with its own sandbox, so it can't write to the app's Documents (where Files and Finder show the Bookmarks). It Saves into a Bookmarks folder in the App Group, the inbox, and the app moves what is there into its Bookmarks (`Bookmarks::take_from`, which keeps when each was Saved, its Tags, Cover and images) whenever it comes to the front or leaves it. The app also publishes its Addresses and the Tags there are to the group, for the extension to find duplicates and suggest Tags. An Add Article for an Address Saved in the app is Saved into the inbox like any page; taking it in gives the app's Bookmark the Article and nothing else.

## Considered Options

- **Moving the Bookmarks folder into the App Group**: no hand-over, but Files and Finder lose the Bookmarks, the Tags folder is shared with the other Contexts, and every install needs a migration.

## Consequences

- A Bookmark Saved from the Share sheet shows in the app once it is in front again, not before.
- Browsers hand over the page's HTML (scripts and styles removed) through `ShareExtension.js`, so that logins and offline work; above `Download.limit` it is downloaded instead, to stay within the extension's memory, which also limits the images downloaded at once.
- App and extension need the App Group, so builds need a provisioning profile (`-allowProvisioningUpdates`).
