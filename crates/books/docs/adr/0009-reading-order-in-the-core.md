# Reading order and Back are decided in the core

Where a Place comes relative to another, where the Table of Contents places a Place, and whether a Jump is still Back are rules every client would need. The core decides them, over Places in their Format's terms (ADR-0006), as it does Book Search (ADR-0008). Clients draw the result. The Table of Contents is built from its entries and answers from a sorted index by binary search, so each Book Search match costs a lookup in log(entries), not a walk over them all. Back is one core object behind a lock, so each scroll is one cheap call; it tells the client when its Back place changes, outside that lock.

## Considered Options

- **Rules in each client**: every platform repeats the PDF point and page tolerances and the Back rules, and they drift apart.
- **Walking the entries per lookup**: simpler, but Book Search would cost matches × entries.
