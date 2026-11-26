---

title: History and undo
description: Every applied plan is one transaction in .cuilo/history; cuilo log lists them and cuilo undo reverts the last one without losing your later edits.
---

Every plan cuilo applies becomes one **transaction**. cuilo journals it in `.cuilo/history` before writing anything, so [`cuilo undo`](/reference/cli/undo/) can put every file and the lockfile back as they were.

## What a transaction records

Each transaction is a TOML file named after its id, such as `.cuilo/history/00000002.toml`. It holds a summary, whether it finished, the SHA-256 of each file before and after, and the lockfile before and after:

```toml
id = 2
time = 1795661478
summary = "update cards 1.0.0 → 1.1.0"
state = "applied"
lock_before = "sha256:019b8b0575c1bbe1aa9715239b25772f86809bbb06b5f4963423f50b82fbfa20"
lock_after = "sha256:3cab1264f32acd1a05cd601d4138a4719e33e38d0bdccfbbc20f58041d544dc2"

[[change]]
path = "src/cards/profile-card.tsx"
before = "sha256:102a79a37f7bf3eb1a4652027657f86321cafc16a94d5fdab0461e84a8a68518"
after = "sha256:d047e9988205f56ab0bbd1a1b27b4f5340d6dc8fb84963a0cf7c4c9381878279"

[[change]]
path = "src/app.tsx"
before = "sha256:02406feece435d3a980369d5c9da74b96f84fe3af9a628efd7aa16b82de7dd95"
after = "sha256:2264dd961c2677f19322a5195974cadc59297f8e1a063702354592b2473eef8f"
```

A missing `before` means the file did not exist, and a missing `after` means the transaction deleted it. A transaction that created folders lists them in `created_dirs`. The content behind each hash is stored in `.cuilo/objects/`, which also keeps the originals that [updates](/guides/updates/) merge from.

`.cuilo/` holds its own `.gitignore`, so Git ignores it. History is local to your copy of the project. Deleting the folder is safe, but you lose undo, and updates have to rebuild originals from the old pack version.

## How applying works

Only one cuilo process changes a project at a time. While it works, it holds `.cuilo/busy`. Before writing, cuilo checks that every file still has the content the plan saw. If you changed one in between, it stops:

```text
error: `src/cards/profile-card.tsx` changed after the plan was made; plan again
```

Then it saves the transaction as `pending`, writes the files and the lockfile, and marks the transaction `applied`. If a write fails, cuilo puts back what it already wrote and drops the entry. If the process is killed partway, the entry stays `pending`, and the next apply refuses to start:

```text
error: transaction 2 was interrupted; run undo to roll it back first
```

`cuilo undo` then rolls the interrupted transaction back.

## List transactions

[`cuilo log`](/reference/cli/log/) lists the transactions you can undo, newest first, with how many files each changed:

```sh
cuilo log
```

```text
   2  update cards 1.0.0 → 1.1.0  (2 file(s))
   1  add cards/card  (1 file(s))
```

`cuilo adopt` and some updates change no files and only write the lockfile, so they show `(0 file(s))`. With nothing to undo, `cuilo log` prints `Nothing to undo.` Use `--json` for the full entries.

## Undo the last transaction

`cuilo undo` reverts the most recent transaction:

```sh
cuilo undo
```

```text
Undid transaction 2: update cards 1.0.0 → 1.1.0
```

It restores each file's previous content, deletes files the transaction created, restores the lockfile, and removes folders the transaction created if they are now empty. Run it again to undo the transaction before that one. The undone entry moves to `.cuilo/history/undone/`. There is no redo, and new transactions keep counting from the highest id.

## What undo refuses

### Files you changed afterwards

Undo checks that every file, and `cuilo.lock`, still holds what the transaction left there. If you edited a file since, undoing would throw your edit away, so it refuses and lists the files:

```text
error: these files changed after transaction 1; undo with force to discard the edits: src/cards/profile-card.tsx
```

`cuilo undo --force` undoes it anyway, and your edits to those files are lost. Commit or copy them first if you want to keep them.

### Paths cuilo never writes

The journal lives in the project folder, so a project you get from someone else, such as an archive, could carry an entry they wrote by hand. Undo refuses any entry that names a path no pack may write, such as `.git/`, even with `--force`:

```text
error: history entry 2 names `.git/hooks/pre-commit`, which cuilo never writes; delete the entry from `.cuilo/history` if you did not create it
```

### Missing content or another process

Undo stops if content it needs is gone from `.cuilo/objects/`, and when another cuilo process holds the project:

```text
error: another cuilo process is changing this project (remove `.cuilo/busy` if it crashed)
```

## What history does not cover

Undo only reverts what cuilo wrote. Packages that a package manager added after you applied a plan are outside the history, so `cuilo undo` leaves them. See [Packages a piece needs](/guides/dependencies/). Edits you make by hand are not transactions either.
