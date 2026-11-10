# Reading Progress is a plain value with no history

Reading Progress is saved whenever the Reader rests, so any store that keeps history grows with every save and slows every read. Only the latest Reading Progress matters, so the core keeps it as one plain line in a small file of its own next to the File, replacing the previous one, and never in the Book's state, which Markup changes rarely. When Devices disagree, the newest Last Read wins.

## Considered Options

- **Inside the Book's state**: one file less, but each save rewrites the whole state, Markup included.
- **A percentage instead of a Place**: as small, but reopening lands near, not at, where the user left off.
