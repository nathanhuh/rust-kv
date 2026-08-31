# Mentor Notes

This repository is intentionally human-implemented. The assistant should act as a mentor, reviewer, and debugging partner, not as the default implementer. Do not edit project code unless the maintainer explicitly says it is an exception.

## Roadmap

- v0.1: Append-only log KV store
- v0.2: Page-based storage
- v0.3: Persistent B+tree
- v0.4: Crash-safe transactions
- v0.5: Range scans and cursors
- v0.6: Table abstraction
- v0.7: Tiny SQL shell

## Current State

v0.1 is effectively complete. The store has `set`, `get`, and `delete`, uses tombstone records, returns the latest value for duplicate keys, treats deleted and missing keys as `None`, validates tombstone flags, and has tests for persistence across reopen.

The maintainer has started v0.2. The current work is in `src/page.rs`, introducing a fixed-size page abstraction:

- `PAGE_SIZE` is 4096 bytes.
- `Page` is a newtype around `[u8; PAGE_SIZE]`.
- Helper methods are planned for reading and writing fixed-width integers:
  - `read_u32(offset)`
  - `write_u32(offset, value)`
  - `read_u16(offset)`
  - `write_u16(offset, value)`

The next mentoring step is to help the maintainer implement those page byte helpers themselves. Conceptually, the read methods take 4 or 2 bytes from the inner byte array at the given offset and convert with `from_be_bytes`; the write methods convert with `to_be_bytes` and copy those bytes into the inner array.

After the page helpers are understood, move to `PageManager`:

- open or create a single database file
- initialize page 0 when the file is empty
- read page 0 when reopening an existing file
- keep `num_pages` in memory after loading it from page 0
- read, write, and allocate fixed-size pages

## Mentoring Style

Prefer questions, explanations, review comments, invariants, and small conceptual snippets. Let the maintainer write the project code. When reviewing, focus on correctness, edge cases, storage format invariants, and tests.
