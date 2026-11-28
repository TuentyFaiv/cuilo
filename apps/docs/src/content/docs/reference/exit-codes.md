---

title: Exit codes
description: The four exit codes cuilo returns, when each one happens, and what it prints, for scripts, CI and AI agents.
---

Every command ends with one of four exit codes, so scripts and agents can tell a finished command from a question cuilo could not ask.

| Code | Meaning                       |
| ---- | ----------------------------- |
| `0`  | Done                          |
| `1`  | Failed                        |
| `2`  | Usage, or confirmation needed |
| `3`  | Plan rejected                 |

## 0: done

The command did what you asked, or there was nothing to do. This includes:

- A plan shown with `--dry-run`, and a plan with nothing to change.
- A plan you declined at `Apply these changes?` in a terminal. cuilo prints `Nothing was changed.`
- Checks you declined at the `--verify` question in a terminal. cuilo prints `Nothing was run or changed.`
- `--help`, `--version`, and `cuilo` with no command, which prints the help.

## 1: failed

Something went wrong, and cuilo prints `error: <message>` on stderr. With `--json`, it prints `{ "error": "<message>" }` on stdout instead. Common causes:

- Inputs that do not fit the piece, such as ``input `name` is required`` or ``input `size` must be one of: small, large``.
- A pack or piece that is not loaded, such as ``no pack `acme-ui` is loaded; use --from <source> to load it``.
- A source that cannot be loaded or verified: a bad source string, a failed download, a pack that does not validate, a refused signature, a source whose files changed since `cuilo.lock` pinned them, or a pack that takes an official name.
- A patch that cannot apply, such as a missing file or an anchor that is missing or not unique.
- A user or project config that cannot be read.
- `--review` without a terminal.
- `cuilo undo` when the files changed since that transaction, unless you pass `--force`.
- A package manager command that fails after the plan was applied with `--install`. The generated files stay applied; add the packages yourself.

## 2: usage, or confirmation needed

The command line is invalid, or the command needs an answer it cannot get:

- An unknown flag, a missing argument or a bad value. The argument parser prints the problem and a usage line.
- A plan that needs your approval without a terminal, or with `--json`. cuilo prints `Nothing was changed; pass --yes to apply without asking, or --dry-run to only plan.`, or with `--json` adds `"error": "pass --yes to apply"`. Nothing is written.
- `--verify` without `--yes` and without a terminal, or with `--json`. cuilo prints `Nothing was run or changed: --verify runs code from the pack as you; pass --yes to run the checks.` Nothing runs.

Run the command again with `--yes`, after reading the plan with `--dry-run` if you need to.

## 3: plan rejected

cuilo planned the change and refused to apply it. Nothing is written. In text mode it lists each problem on stderr:

```text
The plan was rejected:
  lib/cards/team-card/team-card.tsx: the pack does not declare permission to write here
```

With `--json`, it prints the problems on stdout:

```json
{
	"error": "plan rejected",
	"problems": [
		{
			"kind": "not_declared",
			"message": "the pack does not declare permission to write here",
			"path": "lib/cards/team-card/team-card.tsx"
		}
	]
}
```

Each problem has one of these kinds:

| `kind`         | Why                                                                                                                       |
| -------------- | ------------------------------------------------------------------------------------------------------------------------- |
| `conflict`     | A file exists with other content (`--force` replaces it), changed while planning, or an update conflicts with your edits. |
| `not_declared` | The pack's `permissions.write` does not cover the path.                                                                   |
| `not_granted`  | You did not grant the pack permission to write there, or to add dependencies.                                             |
| `forbidden`    | No pack may write there, such as `.git/`, `.cuilo/` or `cuilo.lock`.                                                      |
| `unsafe`       | The path goes through a symlink or a file.                                                                                |
| `unavailable`  | A tool the plan needs is missing, such as `ast-grep` for a migration.                                                     |

`--verify` also exits with `3` when a check fails because of the plan. cuilo prints `Nothing was changed: a check fails with this plan.`, or with `--json` the plan and the check results with `"applied": null`. See [verify](/guides/verify/).

For update conflicts, choose another `--on-conflict` value: `markers`, `ours` or `theirs`. See [updates](/guides/updates/).
