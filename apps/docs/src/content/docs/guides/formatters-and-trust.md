---

title: Formatters and trust
description: cuilo runs your project's own formatter and fixing linters on generated files, but only in projects you trust with cuilo trust.
---

Generated files follow your project's formatter and fixing linters, so they pass its checks as if you had written them. cuilo never installs these tools. It uses the copy your project already has, and only in a project you trust.

## Which tools run

| Tool                                   | Detected by                                               | Runs                           |
| -------------------------------------- | --------------------------------------------------------- | ------------------------------ |
| ESLint (fixes), Biome, Prettier, oxfmt | its package in `package.json` or its config               | from `node_modules/.bin`       |
| dprint                                 | `dprint.json`                                             | `node_modules/.bin`, else PATH |
| Ruff (fixes, then format), Black       | `[tool.ruff]` / `[tool.black]`, `ruff.toml`, dependencies | from `.venv`, else PATH        |
| rustfmt, gofmt                         | a Rust or Go project                                      | from PATH, with `rustfmt.toml` |

They run in this order: ESLint, Ruff, Biome, Prettier, oxfmt, dprint, Black, rustfmt, gofmt. Fixing linters go first, so a formatter has the last word. Each tool gets only the file types it handles: Ruff and Black get Python files, rustfmt gets `.rs`, gofmt gets `.go`, and the JavaScript tools get scripts plus the other types they format, such as CSS or JSON.

cuilo runs them from the project root while it plans, on each file through stdin, so nothing is written before you apply. `--dry-run` already shows the formatted files. The formatted file is also the original that later [updates](/guides/updates/) merge from, so `cuilo status` shows it as `clean`.

To choose the tools yourself, set `stack.formatter` in `cuilo.json`, for example when the project has several. Use `[]` for none. [`cuilo detect`](/reference/cli/detect/) shows what cuilo found.

```json
{
	"stack": { "formatter": ["eslint", "prettier"] }
}
```

## Trust a project

The formatters come from the project, and their config files can run code. So cuilo runs them only in projects you trust. Until then, the plan says what it did not format:

```text
Notes:
  cuilo.lock: not formatted with Prettier: cuilo runs a project's formatters and linters only once you trust it; run `cuilo trust` here
```

In a terminal, `cuilo add`, `cuilo adopt` and `cuilo update` ask once per project, when the project uses a formatter cuilo knows:

```text
This project uses Prettier. Let cuilo run them on the files it generates? They come from the project and run as you; your answer is remembered (`cuilo trust` changes it). [y/N]
```

Both answers are remembered. [`cuilo trust`](/reference/cli/trust/) sets the answer without asking:

```sh
cuilo trust
```

```text
Trusted. cuilo will run Prettier on the files it generates here.
```

`--no-input`, `--json` and MCP sessions never ask, so they rely on `cuilo trust`. The same trust lets [`--verify`](/guides/verify/) run the project's checks.

cuilo keeps the answers in `trusted-projects.toml`, next to your [user config](/reference/user-config/): in `$CUILO_CONFIG_DIR`, else `$XDG_CONFIG_HOME/cuilo` or `~/.config/cuilo`, or `%APPDATA%\cuilo` on Windows. Projects are listed by their full path, so a project moved to another folder is asked about again.

### rustfmt and gofmt

rustfmt and gofmt come from your system, not the project, but they need trust too. Project files can choose which one runs, as `rust-toolchain.toml` does for rustup. rustfmt also reads the project's `rustfmt.toml` and gets the edition from `Cargo.toml`.

### Set it for every project

`[tools] trust` in your user config skips the question everywhere:

```toml
[tools]
trust = "always" # or "never"; the default is "ask"
```

This setting comes first. With `"never"`, no project's tools run, even after `cuilo trust`.

## Revoke trust

```sh
cuilo trust --revoke
```

```text
cuilo will no longer run this project's own tools.
```

Revoking also reaches `cuilo mcp` and `cuilo rpc` sessions that are already running. They check trust again on each call, so they stop running the project's tools without a restart.

## When a tool cannot run

The plan notes each problem, and the files are planned without that tool:

```text
Notes:
  cuilo.lock: Prettier is set up in this project but not installed, so generated files were not formatted with it; install the project's dependencies
```

```text
Notes:
  src/cards/profile-card.tsx: Prettier could not format this file: [error] src/cards/profile-card.tsx: SyntaxError: Unexpected token (3:1)
```

A name in `stack.formatter` that cuilo does not support gets a note too:

```text
Notes:
  cuilo.lock: cuilo cannot run the formatter `stylua`; supported: eslint, ruff, biome, prettier, oxfmt, dprint, black, rustfmt, gofmt
```
