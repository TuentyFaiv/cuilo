# cuilo

> **Status: v2 rewrite, usable locally.** Nothing is published from this branch yet; install it from source to try it.
> v1 (`@tuentyfaiv/cli` ≤ 0.0.11) is being replaced by Cuilo (`@cuilo/cli`); see the migration notes once v2 ships.

cuilo generates projects and code from **packs** — for humans at the terminal and for AI agents over MCP. A pack is
a versioned bundle of starters, pieces and presets that anyone can publish from a git tag, a tarball URL or a local
folder. Code a pack adds is yours to edit, and `cuilo update` still merges upstream fixes into it. Pieces adapt to the
project's stack, declare typed inputs (prompts, flags and MCP schema from one definition) and can patch existing files.
Packs are verified by hash and optional signature, installs are dependency-free, and every command works
non-interactively.

## Try it

```sh
cargo install --path crates/cuilo --locked   # puts `cuilo` in ~/.cargo/bin

cd my-react-or-svelte-app
cuilo pack list                       # official packs: ui (atoms, molecules, organisms), backend
cuilo add ui/atom Button --dry-run    # see the plan; nothing is written
cuilo add ui/atom Button              # asks before applying; --yes skips the question
cuilo status                          # clean / modified / missing per generated file
cuilo why src/ui/atoms/button/button.tsx:3
cuilo undo                            # reverts the last apply, refusing to drop later edits
```

More commands:

| Command                                                         | What it does                                                                      |
| --------------------------------------------------------------- | --------------------------------------------------------------------------------- |
| `cuilo add <pack/piece> [name] -i key=value --from <source>`    | Generate a piece, loading its pack from a source first                            |
| `cuilo adopt <pack/piece> [name]`                               | Take over existing code so updates merge into it                                  |
| `cuilo update <pack> [--from <source>] [--on-conflict markers]` | Move to a new pack version, merging your edits                                    |
| `cuilo pack add <source>`                                       | Load and verify `gh:`, `git:`, `https://`, `registry:`, `./` packs                |
| `cuilo pack capture <files…> --piece <name> --name UserCard`    | Turn project code into a piece of `packs/local`; the name needs two or more words |
| `cuilo log`, `cuilo detect`, `cuilo pack show <pack>`           | History, detected stack, a pack's pieces and inputs                               |
| `cuilo trust [--revoke]`                                        | Let this project's own formatter, linters and checks run (formatting, `--verify`) |
| `cuilo upgrade [--check] [--version <v>]`                       | Upgrade cuilo, through the package manager that installed it, if any              |

Every command takes `--json` and `--no-input` for scripts and agents; `--dry-run`, `--diff`, `--review`
(keep or skip each hunk, like `git add -p`), `--verify` and `--install` work wherever a plan is applied. Exit
codes: `0` done, `1` failed, `2` usage or confirmation needed, `3` plan rejected (also by `--verify`).
`cuilo completions <bash|zsh|fish|powershell|elvish>` prints a completion script.

### Installing a release

Releases are not published yet. Once they are, every channel installs the same binaries, built and attested by
GitHub Actions and served from Cloudflare under `/v<version>/`; [RELEASING.md](RELEASING.md) describes how they are
made and what protects them.

| Channel              | Command                                                                       | What checks it                                      |
| -------------------- | ----------------------------------------------------------------------------- | --------------------------------------------------- |
| npm, pnpm, bun, yarn | `npm install --global @cuilo/cli`                                             | Lockfile integrity, npm provenance                  |
| macOS, Linux         | `curl --proto '=https' --tlsv1.2 -fsSL https://dl.cuilo.dev/install.sh \| sh` | SHA-256, and the signature when `minisign` is there |
| Windows              | `irm https://dl.cuilo.dev/install.ps1 \| iex`                                 | SHA-256, and the signature when `minisign` is there |
| Homebrew             | `brew install tuentyfaiv/tap/cuilo`                                           | The formula's SHA-256                               |
| cargo                | `cargo binstall cuilo` or `cargo install cuilo --locked`                      | The signature (binstall), crates.io checksums       |

The npm package has no dependencies and runs
nothing at install time: the package manager picks the one platform package (`@cuilo/cli-darwin-arm64`,
`…-linux-x64-musl`, …) that matches the system. The install scripts put `cuilo` in `~/.cuilo/bin`
(`CUILO_INSTALL` changes it; `CUILO_VERSION` or an argument picks a version, else the newest stable one), and
`CUILO_REQUIRE_SIGNATURE=1` makes them refuse to install without checking the signature. Only those copies update
themselves, once a day in the background or with `cuilo upgrade`, and only when the release's `SHA256SUMS` carries a
valid signature by the release key built into cuilo; without `--version`, they never move to an older version. For
global installs made another way, `cuilo upgrade` runs their package manager, and the daily check prints a notice
instead; for a project's own copy or one run through `npx`, `bunx`, `pnpm dlx` or `yarn dlx`, it prints the command
to run instead of running anything. `updates = "notify"` or `"off"` in the user config, or `CUILO_UPDATES`, changes this, and
`cuilo upgrade --check` only reports what is available. To see where a binary was built, run
`gh attestation verify $(command -v cuilo) --repo TuentyFaiv/cuilo`.

Every release file is signed with this minisign key (also in
[`crates/cuilo/release-key.pub`](crates/cuilo/release-key.pub)):

```text
RWTFcgVmt+E7Xi77vi6Q/Yu68TJDTsBTKYHqOmyjSOY4VCgpEFCwl+dK
```

### Checking a plan before it lands

`--verify` applies the plan to a throwaway copy of the project (without `.git`, build output and caches;
`node_modules` and `.venv` are linked, not copied) and runs the project's own checks there: its `typecheck`,
`type-check`, `check`, `lint` and `test` scripts (else `build`), `cargo check --all-targets`, `go vet ./...`, and
Ruff and pytest when the project uses them. Set `"verify": [["bun", "run", "check"]]` in `cuilo.json` to choose
the commands. The plan is applied only when no check fails because of it. A check that fails runs again without the
plan: if it fails the same way, it can't judge this plan and doesn't block it; if the plan adds new problems,
those lines are shown and the plan is refused. The checks are the project's code, so `--verify` needs a trusted
project (see below). They also run with the plan's files in place, so code from the pack, such as a `build.rs` or
a test, runs as you: cuilo prints the plan first and starts the checks only after you agree, or with `--yes`
(also with `--dry-run`). A plan that writes into the linked `node_modules` or `.venv` is refused, since the copy
shares those folders with the project. The copies live in the `verify` folder of your cuilo cache, which only you
can open; cuilo refuses a cache folder that other users can write to, such as one in `/tmp`, because the checks'
tools read config files from the folders above them.

### Packages a piece needs

Plans list the packages their pieces import. After applying, cuilo offers the command that adds the missing
ones with the project's package manager, e.g. `bun add -d vitest@^4.0`, `cargo add tokio@1 --features macros`,
`uv add 'httpx>=0.28'`, `go get …`, `dart pub add …`, `bundle add …` or `composer require …`. Packages the
project already declares are skipped, so their versions never change. `--install` runs it without asking and
`--no-install` never offers it; otherwise, without a terminal, the command is printed to run yourself. Changes the
package manager makes are outside cuilo's history, so `cuilo undo` doesn't revert them.

For AI agents, `cuilo mcp` serves the same plan → review → apply flow over MCP, e.g.
`claude mcp add cuilo -- cuilo mcp`. Nothing is written until the agent calls `apply_plan`, and `verify_plan`,
which runs code from the pack as `--verify` does, is marked as not read-only so agents ask you first.

### Your formatter and linter

Generated files follow the project's own formatter and fixing linters, so they pass its checks as if you had
written them. cuilo never installs these tools; it uses the copy the project already has:

| Tool                                   | Detected by                                  | Runs                           |
| -------------------------------------- | -------------------------------------------- | ------------------------------ |
| ESLint (fixes), Biome, Prettier, oxfmt | its package in `package.json` or its config  | from `node_modules/.bin`       |
| dprint                                 | `dprint.json`                                | `node_modules/.bin`, else PATH |
| Ruff (fixes, then format), Black       | `[tool.ruff]` / `[tool.black]`, dependencies | from `.venv`, else PATH        |
| rustfmt, gofmt                         | a Rust or Go project                         | from PATH, with `rustfmt.toml` |

They run in that order, from the project root, on each file while planning (on stdin, so nothing is written before
you apply), and the plan notes any file a tool could not handle. The formatted file is the original later updates
merge from. Set `stack.formatter` in `cuilo.json` to choose the tools, e.g. `["eslint", "prettier"]` when the
project has several, or `[]` for none.

Tools that come from the project, or whose config can run code, run only in projects you trust:
`cuilo add` asks once per project and remembers the answer; `cuilo trust` (or `--revoke`) sets it without
asking, which is what `--no-input`, `--json` and MCP sessions rely on. Until then, plans say what was not formatted.
`[tools] trust = "always"` or `"never"` in your user config skips the question everywhere. rustfmt and gofmt need
trust too: they come from your system, but project files can choose which one runs, as `rust-toolchain.toml` does
for rustup. Revoking trust also stops `cuilo mcp` and `cuilo rpc` sessions that are already running.

## Repository layout

```text
crates/
  cuilo-core/      engine, in layers: model → template → patch → pack → workspace
                     (files, config, lockfile) → history → plan → engine
  cuilo-packs/     pack sources, in layers: spec → transport (HTTPS, git over SSH, credentials)
                     → format (archives, registry items) → signing → store (cache, official
                     packs, library) → resolver
  cuilo-rpc/       stdio JSON-RPC API over the engine (GUI, tools)
  cuilo-mcp/       MCP server over stdio, one tool per engine call
  cuilo/           the CLI: cli (arguments) and ui (output, prompts, review) → inputs and
                     context → commands
  xtask/             repository automation (`cargo xtask <task>`), including the release files
npm/cli/             the npm wrapper package; the platform packages are written at release time
packs/
  ui/                official atomic UI pieces: atom, molecule, organism (web, Rust, native)
  backend/           official backend pieces: resource, error, middleware, database
schemas/             JSON Schemas for pack.toml, piece.toml, cuilo.lock and config
apps/docs/           the cuilo.dev docs site (Astro + Starlight, English and Spanish), served as
                       static files from Cloudflare Workers
install/             install.sh and install.ps1, served from the download host
```

## Development

Requirements: [rustup](https://rustup.rs) (the pinned toolchain in `rust-toolchain.toml` installs automatically) and
[Bun](https://bun.sh) ≥ 1.4.4 for Biome and the docs site.

```bash
bun install            # Biome and the docs site
cargo xtask ci         # everything CI runs: fmt, clippy, rustdoc, tests, cargo-deny, Biome
cargo xtask fmt        # or a single step: fmt | lint | doc | test | deny | js
cargo xtask schema     # regenerate schemas/ after changing a file format
cargo xtask docs       # regenerate the docs site's CLI and schema reference and pack data
bun run docs           # run the docs site locally; bun run docs:build builds it
cargo run -- --help    # run the CLI
```

`cargo xtask deny` needs [cargo-deny](https://github.com/EmbarkStudios/cargo-deny) on your `PATH`.

The docs site deploys through Cloudflare Workers Builds on every push to `main`; its settings are in
[`apps/docs/README.md`](apps/docs/README.md).

### Conventions

- Formatting matches `biome.json`: hard tabs, width 2, LF, 120 columns (`rustfmt.toml` mirrors it).
- Clippy runs with warnings denied; `unwrap`, `expect`, `panic`, `todo` and `dbg!` are errors outside tests.
- Libraries use `thiserror`; only the binary uses `anyhow`.
- Every product name comes from `cuilo_core::brand`, so the planned rename stays a one-line change.

## Writing a pack

A pack is a folder with a `pack.toml` and one folder per piece:

```text
my-pack/
├─ pack.toml                 # name, version, permissions
└─ pieces/component/
   ├─ piece.toml             # inputs, variants, file rules, patches
   └─ files/react/my-example.tsx
```

Templates are **real code**. Each input names an `example` that appears in the templates; cuilo
replaces it in every case form (`MyExample`, `myExample`, `my-example`, `my_example`, `MY_EXAMPLE`,
`my/example`, …) and as written (`com.example.app`), file names included. Sentence case
(`My example`) is not a case form, so write examples in one of those forms. An example needs two or
more words to be replaced in every form; a one-word example is replaced only as written, so ordinary
words such as an HTML `<header>` tag stay untouched. Optional code uses comment directives in any
comment style:

```tsx
import styles from "./my-example.module.css"; // @cuilo if styled

// @cuilo if styled
return <h2 className={styles.myExample} />;
// @cuilo else
return <h2 className="my-example" />;
// @cuilo end
```

A variant can set its own `target` (Flutter writes to `lib/`, Compose to `app/src/main/java/…`); an
empty `target` writes from the project root, for pieces that also add files under `test/` or `tests/`.

Templates cannot read files or the environment, or run code. A pack can only write the paths its
`permissions.write` globs declare, never `.git/` or cuilo's own state (in any letter case, or under another name
the disk gives them), and every plan flags risky
paths such as `package.json`, CI workflows or agent configuration.

A bool input can take its default from the project's stack, so pieces only add what the project can use:

```toml
[inputs.tests]
type = "bool"
default_when = "stack.vitest || !(stack.javascript || stack.typescript)"  # stack.<fact value>
```

A piece can build on other pieces of its pack and name the packages its code imports:

```toml
requires = ["error"]          # added first when the project does not have it yet; it gets the
                              # inputs it shares with this piece, such as `tests`

[[dependencies]]
ecosystem = "npm"             # npm, cargo, pypi, go, pub, rubygems or packagist
name = "zod"
version = "^4.1"
variants = ["hono", "express"]
```

Plans list dependencies and offer to add them with the project's package manager (see "Packages a piece
needs"). Declaring them needs `permissions.dependencies = true` in `pack.toml`, and the user can refuse them.

Patches edit files the project already has: `anchor` inserts text next to a unique line (or the first of a list of
alternatives the file contains, e.g. `anchor = ["export default app", "serve("]`), `import` adds an import, and
`css_vars` adds custom properties to a CSS block, creating the block if needed and keeping values already set:

```toml
[[patches]]
kind = "css_vars"
file = "src/app/globals.css"
selector = ":root"            # or `.dark`, `@theme inline`, …
vars = { sidebar-border = "220 13% 91%" }
```

### Pack sources

| Source                                      | Example                                                                             |
| ------------------------------------------- | ----------------------------------------------------------------------------------- |
| Official (built into the binary)            | `ui`, `official:backend`                                                            |
| Folder                                      | `./packs/mine`, `path:/opt/packs/mine`                                              |
| GitHub tag or commit                        | `gh:acme/packs@v1.2.0`, `gh:acme/packs/ui@v1.2.0`                                   |
| HTTPS tarball                               | `https://example.com/ui-1.2.0.tar.gz`                                               |
| Registry item (`registry-item.json` format) | `registry:https://acme.dev/r/card.json`, `registry:@acme/card`                      |
| Any git repository over SSH                 | `git:git@github.com:acme/packs.git@v1.2.0`, `git:ssh://git@host/x.git//packs/ui@v2` |
| Namespace from the project config           | `@acme/ui`, with `"sources": { "@acme": "gh:acme/packs@v1.2.0" }`                   |

Downloads are HTTPS only and verified against the operating system's trusted certificates. The lockfile
records each pack's source and the SHA-256 of its files: a pinned pack loads from the cache offline, and a
source that now serves different files for the same pin is refused. Network sources come from the `remote` feature of
`cuilo-packs`, on by default; `--no-default-features` leaves out the HTTPS crates and refuses `git:`, and remote
sources then load only from the cache. Official packs ship with cuilo, so a newer cuilo brings their next version;
their names (`ui`, `backend`) are reserved, so a pack from any other source must use another name. A lockfile record
loads only the pack and version it names.

Private sources use credentials from your user config (`config.toml` in the cuilo config folder), never from the
project config, so a cloned project cannot send your tokens elsewhere. Only the name of an environment variable is
stored:

```toml
[auth."github.com"]
env = "GITHUB_TOKEN" # sent as `Authorization: Bearer …`; `header` and `scheme` change that
```

With a token, `gh:` sources download through the GitHub API. Tokens go only to their own host, never to a host a
download redirects to. `git:` sources work over SSH only and run your own `git`, so your SSH keys, agent and
`~/.ssh/config` apply; this is how GitLab, Gitea and self-hosted repositories are reached. cuilo never prompts
(SSH runs in batch mode unless you set your own SSH command), stops git after two minutes, and uses `git archive`,
so nothing is checked out.

### Registry items

`registry:<url>` turns an item in the `registry-item.json` format into a pack named `registry-<item>`: the item and
every item it requires become pieces, and its npm packages are listed in the plan. Required items named without a
URL are fetched from beside the item that requires them. Where files go and how imports read comes from the
project config, with aliases resolved through `tsconfig.json` (or `jsconfig.json`) `paths`, so
`~molecules` lands wherever `~molecules/*` points:

```json
{
	"registry": {
		"aliases": { "components": "@/components", "utils": "@/lib/utils" },
		"css": "src/app/globals.css",
		"server_components": true,
		"namespaces": { "@acme": "https://acme.dev/r/{name}.json" }
	}
}
```

The pack's version comes from the fetched content, so `cuilo update registry-<item> --from <same source>` fetches
the item again and merges it like any other pack. CSS variables go into `registry.css` (`light` in `:root`, `dark`
in `.dark`, `theme` in `@theme inline`), keeping values the project already set; `css`, `tailwind` and `envVars`
are not applied yet, and the pack description names them.

### Signatures

A pack can be signed with [minisign](https://jedisct1.github.io/minisign/): sign the one-line message
`cuilo pack <name> <version> <digest>` and put the signature in `pack.minisig` at the pack root (the signature is
not part of the digest). Trust keys in your user config:

```toml
[signing]
require = true                 # refuse unsigned packs and packs signed by keys you have not trusted
keys = { acme = "RWQ…" }       # minisign public keys
revoked = ["1C3B7AE36BE79191"] # key ids refused even when the signature is valid
```

A bad signature or a revoked key is always refused; cuilo also ships its own revocation list. The lockfile records
the whole public key that signed each pack, not only its key id, and a later version signed by another key, or not
signed, is refused. Two trusted keys with the same key id are refused as a config error. Official packs ship inside
cuilo and need no signature.

### Updating generated code

An update moves every piece from a pack to another version of it, as one plan you review before applying:

| The file since generation       | What the update does                                                         |
| ------------------------------- | ---------------------------------------------------------------------------- |
| Unedited                        | Takes the new version, or removes it if the new version dropped it           |
| Edited on other lines           | Merges both, line by line                                                    |
| Edited on the same lines        | Conflict: refuse (default), write Git-style markers, keep yours, take theirs |
| Edited, and dropped by the pack | Stays as you left it and is no longer tracked                                |
| Deleted by you                  | Stays deleted                                                                |

New files and newly required pieces are added, patch regions cuilo inserted are replaced unless you edited them,
and the merge base is the original cuilo wrote: cached in `.cuilo/`, or rebuilt from the old pack version.
Updates are transactions like any other, so undo restores the previous version and lockfile.

A pack can ship migrations for code it does not own, run when an update crosses their version and shown in the
plan like any other change:

```toml
[[migrations]]
to = "2.0.0"
description = "Button's `kind` prop is now `variant`"
files = ["src/**/*.tsx"]                # must also be in `permissions.write`
ast_grep = "migrations/2.0.0-button.yml" # or: rename = { from = "OldName", to = "NewName" }
```

`rename` is built in and matches whole words. `ast_grep` rules run with [ast-grep](https://ast-grep.github.io)
(`ast-grep` on `PATH`, or `ast_grep = { command = [...] }` in your user config); it reads each file on stdin and
cuilo applies the edits it reports, so ast-grep never writes to the project. Files the update itself changes are
skipped and noted, and an update can run without migrations.

Any plan can be reviewed hunk by hunk before applying, like `git add -p`: keep a file's whole change, none of it,
or only some hunks. A skipped hunk counts as your edit, so later updates keep it skipped; a skipped new file is left
out of the lockfile, so the next update offers it again.

## Official packs

`packs/` holds the default packs; `tests/official_packs.rs` renders every piece in every variant.

- **`ui`**: atomic design pieces in `src/ui/{atoms,molecules,organisms}/<name>/`, with typed props, accessible
  markup, CSS (or Tailwind when detected) and tests, for React, Solid, Qwik, Svelte, Vue, Astro, Angular, Lit,
  Leptos, Dioxus, Yew, Flutter, Jetpack Compose (Android and Multiplatform) and SwiftUI.
- **`backend`**: `resource` (model → repository → service → routes, plus tests), `error` (RFC 9457 problem
  details), `middleware` (request id style headers) and `database`, for Hono, Express, Axum, FastAPI, chi and
  Echo. Repositories follow the detected ORM (Drizzle, Prisma, sqlx, SQLAlchemy, GORM) and always include an
  in-memory one for tests.

File names are kebab-case wherever the language allows it (`user-card.tsx`, `user-card.module.css`). Where the
language or its standard tooling fixes the name, the pack follows it: snake_case for Rust, Python, Go and Dart
modules, and PascalCase for Swift and Kotlin types.

## License

[MIT](LICENSE) © Tonalli López
