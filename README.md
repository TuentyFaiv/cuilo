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
(`CUILO_INSTALL` changes it; `CUILO_VERSION` or an argument picks a version, else the newest stable one, or the newest
pre-release while there is no stable one). Until 2.0 is stable, npm's `latest` tag is the newest pre-release too, but
Cargo installs a pre-release only when it is named: `cargo binstall cuilo --version <version>`.
`CUILO_REQUIRE_SIGNATURE=1` makes the scripts refuse to install without checking the signature. Script copies update
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
fixtures/stacks/     one generated project per stack that `cargo xtask verify-packs` adds the official pieces to
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
cargo xtask verify-packs --stack svelte   # add the official pieces to real projects, run their checks
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

A variant can set its own `target` (Compose writes into its `app` module), or list alternatives in order of
preference, of which cuilo uses the first folder the project has (Compose Multiplatform takes `shared`, else the
older `composeApp`); an empty `target` writes from the project root, for pieces that also add files under `test/`
or `tests/`.

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

`packs/` holds the default packs; `tests/official_packs.rs` renders every piece in every variant, and
`cargo xtask verify-packs` checks the generated code with each stack's own tools.

- **`ui`**: atomic design pieces in `src/ui/{atoms,molecules,organisms}/<name>/`, with typed props, accessible
  markup, CSS (or Tailwind when detected) and tests, for React, Solid 2, Qwik, Svelte, Vue, Astro, Angular, Lit,
  Leptos, Dioxus, Yew, Flutter, Jetpack Compose (Android and Multiplatform) and SwiftUI.
- **`backend`**: `resource` (model → repository → service → routes, plus tests), `error` (RFC 9457 problem
  details), `middleware` (request id style headers) and `database`, for Hono, Express, Axum, FastAPI, chi and
  Echo v4 and v5. Repositories follow the detected ORM (Drizzle, Prisma, sqlx, SQLAlchemy, GORM) and always include an
  in-memory one for tests; `database` sets up the client, connection or pool they use from `DATABASE_URL`.

### Verifying the official packs

`cargo xtask verify-packs` adds every official piece, in every variant, to a copy of a real project of that stack
and runs the stack's own tools on the result: svelte-check, `astro check`, `ng build`, `tsc` or `vue-tsc`,
`cargo clippy -- -D warnings`, `go vet` and `gofmt -l`, strict mypy, `flutter analyze` with infos and warnings
fatal, the Kotlin compiler with `allWarningsAsErrors` (plus Android lint with warnings as errors for Jetpack
Compose), and `xcodebuild build-for-testing` or `swift build` with warnings as errors. Then it runs the tests the
piece wrote (Vitest, `ng test`, `cargo test`, `go test`, pytest, `flutter test`, Robolectric through
`testDebugUnitTest`, Compose's UI tests through `jvmTest`, and Swift Testing through `xcodebuild
test-without-building` on one iOS simulator, without parallel clones so it boots once per run, and `swift test`). The `swiftui-package` tests import the library without `@testable`, so a view or initializer that is
not `public` fails the run. No database is needed: commands see a placeholder `DATABASE_URL`.

```bash
cargo xtask verify-packs --list                        # every combination, its checks and what is not covered yet
cargo xtask verify-packs --stack svelte                # one stack; also --group <group>, e.g. kotlin or swift
cargo xtask verify-packs --piece ui/atom --pack ui     # filters combine
cargo xtask verify-packs --stack hono-prisma --keep    # keep each copy in target/verify-packs after it passes
cargo xtask verify-packs --latest                      # first update every dependency to its newest release
```

For each combination it copies `fixtures/stacks/<stack>` to `target/verify-packs/<stack>/<pack>-<piece>`, installs
the locked dependencies, checks that `cuilo detect` sees the intended stack, runs the workspace's `cuilo add` with
the examples the inputs describe, `tests=true`, `--wire` and `--install`, follows the next steps a user would
(module declarations, mounting routes, `prisma generate`, the Gradle test dependencies), then runs the checks. It stops at the first failure and
prints the command, its output and how to run it again, so a failure reproduces with a copy-paste. A combination
whose toolchain is missing is skipped with the reason; `--require-toolchains`, which CI passes, makes that a
failure. Each group needs its toolchain on the `PATH`:

| Group        | Toolchain                                                                                                              |
| ------------ | ---------------------------------------------------------------------------------------------------------------------- |
| `typescript` | Bun and Node.js; the Svelte tests run in Playwright's Chromium, which the command installs                             |
| `rust`       | the pinned toolchain; `--latest` also needs `cargo upgrade` from [cargo-edit](https://github.com/killercup/cargo-edit) |
| `go`         | Go 1.27                                                                                                                |
| `python`     | uv                                                                                                                     |
| `flutter`    | Flutter 3.50                                                                                                           |
| `kotlin`     | a JDK 17 or newer as `java`, and the Android SDK in `ANDROID_HOME` for `compose`; `--latest` also needs curl           |
| `swift`      | Xcode 27.2 on macOS, with its iPhone 17 simulator                                                                      |

Android Studio's bundled JBR works as the JDK (`JAVA_HOME="/Applications/Android Studio.app/Contents/jbr/Contents/Home"`
with `$JAVA_HOME/bin` on the `PATH`), and Gradle adds the Android platform the `compose` fixture compiles against to
the SDK when it is missing.

`--latest` keeps the few packages each stack lists in `hold` (in `crates/xtask/src/verify/stacks.rs`) at the
fixture's major: TypeScript 7 for the checkers built on its JavaScript API, and Prisma 8. Flutter runs
`flutter pub upgrade --major-versions`, SwiftPM `swift package update`, and an Xcode project resolves its packages
again without its `Package.resolved`. Gradle has no command for it, so `--latest` moves each version in
`gradle/libs.versions.toml` to the newest release of the library or plugin that uses it (a pre-release to the newest
release of any kind, a stable version to the newest stable one), again after the next steps add theirs, and the
wrapper to the newest Gradle.

The [Packs workflow](.github/workflows/packs.yml) runs one job per group, `swift` on GitHub's `xcode-27` macOS
image (`macos-latest` has only Xcode 26) and the rest on Linux, when a change touches the packs, the engine, the
fixtures or the workflow, every night with `--latest` (opening or updating an issue when it fails), and on demand.
Its `packs-ok` job passes when every group passed or none needed to run, so it is the check to require: mark
`packs-ok` as a required status check in the branch protection of `main` and `staging`, a repository setting the
workflow cannot change.

Each fixture comes from its framework's own generator, trimmed of demo code, with its lockfile committed. The Gradle
fixtures pin every version in their version catalog and their wrapper by `distributionSha256Sum`, and the workflow
validates their wrapper jars; the Swift fixtures have no dependencies, so they have no `Package.resolved`. Where the
build compiles only some folders (Flutter's `lib` and `test`, Gradle source sets, the Xcode app's folder, a package's
targets), the stack lists them as `sources`, and a piece that creates a file elsewhere fails the run, since the
checks would never see it.

| Fixture                         | Made with                                                                                                                 |
| ------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| `svelte`                        | `sv@1.7.1 create --template minimal --types ts --add vitest` (component tests in the browser)                             |
| `astro`                         | `create-astro@5.3.2 --template minimal`, plus `@astrojs/check`, TypeScript 6 and Vitest with `getViteConfig`              |
| `leptos`                        | [`leptos-rs/start-trunk`](https://github.com/leptos-rs/start-trunk) at `9bbd060`, stable channel                          |
| `dioxus`                        | [`DioxusLabs/dioxus-template`](https://github.com/DioxusLabs/dioxus-template) `v0.7` at `1308c32`, Bare-Bones for the web |
| `yew`                           | [`yewstack/yew-trunk-minimal-template`](https://github.com/yewstack/yew-trunk-minimal-template) at `68dcc85`, stable      |
| `axum`, `axum-sqlx`             | axum's hello world with `cargo add axum tokio`; then `cargo add sqlx --features runtime-tokio,postgres`                   |
| `angular`                       | `@angular/cli@22.3.0 new --style css --ssr false --defaults` (Vitest), with a one-line welcome page                       |
| `qwik`                          | `create-qwik@1.20.4 empty`, then `qwik add vitest`, without ESLint and Prettier                                           |
| `solid`                         | `create-solid@0.13.0 --solid --ts -t basic --no-ssr --no-devtools` (Solid 2), minus its demo pages and counter            |
| `vue`                           | `create-vue@3.24.0 --ts --vitest --bare`, without the devtools plugin                                                     |
| `react`, `lit`                  | `create-vite@9.4.0 --template react-ts` and `lit-ts`, plus Vitest with jsdom                                              |
| `hono`                          | `create-hono@0.19.5 --template nodejs`                                                                                    |
| `express`                       | Express 5's hello world in TypeScript with `NodeNext` modules (Express has no TypeScript generator)                       |
| `*-drizzle`                     | the framework's fixture plus `drizzle-orm` and `pg`                                                                       |
| `*-prisma`                      | the framework's fixture plus Prisma 7 and `prisma init --output ../src/generated/prisma`, checking `src` only             |
| `fastapi`, `fastapi-sqlalchemy` | `uv init --app --no-package` on Python 3.15 with `fastapi[standard]` and strict mypy; then `sqlalchemy[asyncio]`          |
| `chi`, `echo`, `*-gorm`         | each router's hello world, Echo v4's for `echo`; then `gorm.io/gorm`                                                      |
| `echo-v5`                       | the hello world in the README of Echo v5.4.7                                                                              |
| `flutter`                       | Flutter 3.50.1's `flutter create --empty --project-name cuilo_fixture --platforms android,ios`, minus their folders       |
| `compose`                       | Android Studio 2026.1.3's Empty Activity with AGP 9.5.3, Compose BOM 2026.11.01, compileSdk 37.2 and Gradle 9.9.2         |
| `compose-multiplatform`         | the [KMP wizard](https://kmp.jetbrains.com)'s Desktop target with tests: Compose Multiplatform 1.12.2, Gradle 9.5.1       |
| `compose-multiplatform-legacy`  | `compose-multiplatform` moved back to the wizard's older layout: one `composeApp` module, the desktop app included        |
| `swiftui`                       | Xcode 27.2's iOS App template (SwiftUI, Swift Testing, no storage), without the UI test target                            |
| `swiftui-package`               | Swift 6.4.2's `swift package init --type library`, plus the iOS 17 and macOS 14 `platforms` SwiftUI needs                 |

Android Studio and Xcode have no command-line generator, so `compose` and `swiftui` are written out from the
templates the apps install: the strings in Android Studio's `wizard-template.jar` and the settings in Xcode's
`.xctemplate` files. `compose` leaves out the launcher icons, backup rules and instrumented test, and
`compose-multiplatform` the wizard's daemon JVM pin, so Gradle runs on the JDK it is given.
`compose-multiplatform-legacy` has the same versions and code in the layout the wizard and JetBrains'
[KMP-App-Template](https://github.com/Kotlin/KMP-App-Template) used before they renamed `composeApp` to `shared`: no
`desktopApp`, with `composeApp` running the desktop app itself. All three add
`allWarningsAsErrors`, and `compose` Android lint with warnings as errors, without the checks that only report newer
releases or the icons it leaves out. The wizard call is `generateKmtProject` with `name=CuiloFixture`,
`id=com.example.fixture` and the spec
`{"template_id":"kmt","targets":{"desktop":{"ui":["compose"]}},"include_tests":true}`.

To refresh a fixture, run its generator again in a scratch folder outside the Cargo workspace (`cargo new` inside it
adds a workspace member), apply the same trims, and replace `fixtures/stacks/<stack>`. Rust fixtures keep their own
`[workspace]` table so Cargo treats them as separate projects. Refresh only the lockfile with the stack's own
command in the fixture (`bun update`, `cargo update`, `go get -u ./... && go mod tidy`, `uv lock --upgrade`,
`flutter pub upgrade` or `swift package update`), or for Gradle the catalog's versions and `./gradlew wrapper
--gradle-version <version> --gradle-distribution-sha256-sum <sum>`, run twice so the jar matches too, then run
`cargo xtask verify-packs --stack <stack>`. Biome skips `fixtures/`, and Cargo and cargo-deny only see the
workspace in `crates/`.

File names are kebab-case wherever the language allows it (`user-card.tsx`, `user-card.module.css`). Where the
language or its standard tooling fixes the name, the pack follows it: snake_case for Rust, Python, Go and Dart
modules, and PascalCase for Swift and Kotlin types.

## License

[MIT](LICENSE) © Tonalli López
