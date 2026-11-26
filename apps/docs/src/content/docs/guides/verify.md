---

title: Verify a plan
description: Run your project's own checks on a private copy with the plan applied, and apply the plan only when no check fails because of it.
---

`--verify` tests a plan before it touches your project. cuilo copies the project, applies the plan to the copy, and runs the project's own checks there. The plan is applied only when no check fails because of it. It works with `cuilo add`, `cuilo adopt`, `cuilo update` and `cuilo pack capture`.

```sh
cuilo add ui/atom Button --verify
```

## Which checks run

If `cuilo.json` has a `verify` list, cuilo runs exactly those commands, in order:

```json
{
	"verify": [["bun", "run", "check"]]
}
```

Otherwise cuilo detects the checks:

| Project                | Checks                                                                                                                                    |
| ---------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| `package.json` scripts | `typecheck`, `type-check`, `check`, `lint` and `test`, else `build`, run with `bun run`, `pnpm run`, `yarn run`, `deno task` or `npm run` |
| Rust                   | `cargo check --all-targets --quiet`                                                                                                       |
| Go                     | `go vet ./...`                                                                                                                            |
| Python with Ruff       | `ruff check .`                                                                                                                            |
| Python with pytest     | `uv run pytest -q` in a uv project, else `pytest -q`                                                                                      |

Ruff and pytest run from `.venv/bin` when the project has them there. Each check runs in the copy with `CI=1` and stops after 10 minutes. Cargo builds into a folder in your cuilo cache, kept between runs so later checks are faster.

If there is nothing to run, cuilo stops:

```text
error: verify: no checks found: add scripts like `typecheck`, `lint` or `test`, or a `verify` list in the project config
```

## How a failure is judged

A check that fails with the plan runs again on a fresh copy without the plan. That tells cuilo whether the plan caused the failure. The results print under `Checks on a copy with the plan applied:`, one line per check with how long it took:

| Label   | Meaning                                                                                                     |
| ------- | ----------------------------------------------------------------------------------------------------------- |
| `pass`  | The check passed with the plan                                                                              |
| `FAIL`  | The check passes without the plan, or fails without it too but prints new problems with it. Blocks the plan |
| `fails` | The check fails the same way without the plan, so it cannot judge this plan. Does not block                 |

For a `FAIL`, cuilo prints the lines the plan added, or the last 40 lines of the output. Then it stops and exits with code `3`:

```text
Nothing was changed: a check fails with this plan.
```

With `--json`, the result has `plan`, `verify` and `applied: null`.

A check whose program is not installed stops the verification with an error such as ``the check `go vet ./...` needs `go`, which is not installed``.

## Why it asks first

The checks are your project's code, and they run with the plan's files in place. Files from the pack, such as a test or a `build.rs`, can run as you. So cuilo prints the plan first and asks before running anything:

```text
--verify runs the project's checks on a copy with these files in place, so code from the pack can run as you. Run the checks? [y/N]
```

Without a terminal, or with `--no-input`, nothing runs unless you pass `--yes`. cuilo exits with code `2`:

```text
Nothing was run or changed: --verify runs code from the pack as you; pass --yes to run the checks.
```

`--json` never asks, so it always needs `--yes`. Without it, the JSON result has `"verify": null` and an `error` that says to pass `--yes`. `--dry-run` with `--verify` also needs `--yes`: it runs the checks and applies nothing.

With `--yes`, cuilo runs the checks and, if they pass, applies the plan without asking. In a terminal without `--yes`, it asks `Apply these changes? [y/N]` after the checks pass.

## Trust

`--verify` runs the project's own commands, so it works only in a project you trust. In a terminal, cuilo asks before planning, the first time:

```text
--verify runs this project's own checks (its scripts, cargo, go, pytest…), as you. Trust this project? Your answer is remembered (`cuilo trust` changes it). [y/N]
```

If you said no, or there is no terminal to ask in, the verification stops:

```text
error: verify: verification runs this project's own checks, so it needs a trusted project; run `cuilo trust`
```

See [Formatters and trust](/guides/formatters-and-trust/).

## The copy

The copy leaves out `.git`, `.cuilo` and build output and caches: `target`, `dist`, `build`, `out`, `coverage`, `.next`, `.nuxt`, `.astro`, `.svelte-kit`, `.turbo`, `.cache`, `.output`, `.vercel` and `__pycache__`, at any depth. A project with more than 50,000 files or 1024 MB outside those folders is refused.

`node_modules`, `.venv` and `venv` are linked into the copy, not copied, so the checks use your installed dependencies. Because the copy shares those folders with your project, a plan that writes into one of them is refused.

The copies live in the `verify` folder of your cuilo cache:

| System         | Cache folder                                   |
| -------------- | ---------------------------------------------- |
| Any            | `$CUILO_CACHE_DIR`, when set                   |
| macOS          | `~/Library/Caches/cuilo`                       |
| Linux and Unix | `$XDG_CACHE_HOME/cuilo`, else `~/.cache/cuilo` |
| Windows        | `%LOCALAPPDATA%\cuilo`                         |

On Unix, cuilo creates the folder so only you can open it, and refuses one that other users can open. It also refuses a folder inside one that every user can write to, such as `/tmp`, because the checks' tools read config files from the folders above them. The error ends with `which every user can write to; set CUILO_CACHE_DIR to a folder of your own`.

Each copy is removed when the verification ends. Copies that an interrupted run left behind are removed by a later run after 24 hours.

## For agents

Over MCP, `verify_plan` does the same job. It is marked as not read-only, so agents ask you before running it. See [MCP](/agents/mcp/).
