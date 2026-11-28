---

title: Security
description: How cuilo limits what packs, projects and agents can do, what it checks before it writes or runs anything, and how to report a vulnerability.
---

cuilo runs code and writes files on your behalf, with packs written by other people. This page describes what it checks, what it never does, and where its limits are.

## Plan first

Every command that changes the project builds a plan first. Nothing is written until you apply it.

- Templates cannot read files or the environment, or run code.
- Formatters run while planning, on each file's text through stdin, so nothing is written before you apply.
- `--dry-run` shows the plan and stops. At a terminal, cuilo asks before it applies unless you pass `--yes`. With `--no-input` or `--json` and no `--yes`, nothing is applied and the exit code is `2`.
- An apply is one transaction, journaled in `.cuilo/history`. If a file changed after the plan was made, the apply stops and nothing is written; plan again.
- Only one cuilo process changes a project at a time. It holds `.cuilo/busy` while it works.
- cuilo never writes through symlinks, and never reads project files through them. The one exception is [import style detection](/guides/stack-detection/#relative-imports-in-generated-code): to read `tsconfig.json` files and the `package.json` files that resolve their `extends`, cuilo follows symlinks and Windows junctions, such as the ones pnpm makes in `node_modules`, but only while every link stays inside the project. cuilo follows these links itself, one folder at a time, and never looks at a path outside the project, so a link cannot make it touch another folder or a network share. Paths cannot leave the project: absolute paths, `..` segments and backslashes are refused. cuilo reads and writes only regular files, never FIFOs, sockets or devices.

Agents get the same rule over MCP: only `apply_plan` writes a plan. See [Connect an agent over MCP](/agents/mcp/).

## What a pack may write

A pack declares the paths it may write as globs in `permissions.write` in its `pack.toml`. A plan that writes anywhere else is rejected, with exit code `3`. A pack that lists packages for the project to install also needs `permissions.dependencies = true`. See the [pack manifest](/reference/pack-manifest/).

Some paths no pack may write, even when it declares them:

- `.git`, the project's Git folder
- `.cuilo`, cuilo's own state
- `cuilo.lock`, the lockfile

These stay forbidden in any letter case, such as `.GIT/config` or `Cuilo.LOCK`, and under another name the disk gives the same entry, such as a Windows short name like `GIT~1` or a hard link to the lockfile.

## Risky paths

Some files run as code, change how the project builds, or steer AI agents. Every plan flags a change to one of them with the reason, in any letter case. The flag does not block the plan. It asks you to read that change closely.

| Path                                                                     | Why the plan flags it                    |
| ------------------------------------------------------------------------ | ---------------------------------------- |
| `**/package.json`                                                        | package scripts run on install           |
| `**/.npmrc`                                                              | changes how packages install             |
| `**/*.config.js`, `**/*.config.ts`, `**/*.config.mjs`, `**/*.config.cjs` | build config runs as code                |
| `**/build.rs`                                                            | Cargo build scripts run as code          |
| `**/Cargo.toml`                                                          | changes Rust dependencies and build      |
| `**/pyproject.toml`                                                      | changes Python dependencies and build    |
| `**/Makefile`                                                            | build recipes run as code                |
| `**/Dockerfile`                                                          | container build runs as code             |
| `.github/**`                                                             | CI workflows run with repository secrets |
| `.gitlab-ci.yml`                                                         | CI pipelines run with repository secrets |
| `.husky/**`                                                              | git hooks run on commit                  |
| `.vscode/**`                                                             | editor tasks and settings can run code   |
| `.idea/**`                                                               | editor settings can run code             |
| `.claude/**`, `.cursor/**`, `.codex/**`                                  | AI agent configuration                   |
| `**/AGENTS.md`, `**/CLAUDE.md`                                           | AI agent instructions                    |
| `**/.env*`                                                               | may hold secrets                         |

In `--json` output and over MCP, each flagged change carries this reason in its `risk` field.

## `--verify` and the private copy

`--verify` applies the plan to a throwaway copy of the project and runs the project's own checks there. The [verify guide](/guides/verify/) covers which checks run.

The checks run with the plan's files in place, so code from the pack, such as a `build.rs` or a test, runs as you. That is why:

- cuilo prints the plan first and starts the checks only after you agree, or with `--yes`. This holds with `--dry-run` too. Without a terminal and without `--yes`, nothing runs and the exit code is `2`.
- `--verify` needs a project you trust, since the checks are the project's own code.
- Over MCP, `verify_plan` is marked as not read-only, so clients that honor annotations ask you first.

The copy is kept private:

- Copies live in the `verify` folder of your cuilo cache. On Unix, each copy can be opened only by you.
- cuilo refuses a cache folder that other users can open, and one inside a folder every user can write to, such as `/tmp`. The checks' tools, such as Cargo, read config files from the folders above them. Set `CUILO_CACHE_DIR` to a folder of your own if you need to.
- The copy leaves out `.git`, `.cuilo`, build output and caches. `node_modules`, `.venv` and `venv` are linked, not copied, so a plan that writes into them is refused.
- Each copy is removed when the verification ends. A copy that an interrupted run left behind is removed by a later verification once it is 24 hours old.

:::caution
The copy is not a sandbox. The checks run as you, with your environment variables and network access. cuilo stops a check after 10 minutes. Use `--verify` only with packs you would let run code on your machine.
:::

## Project trust

Formatters and fixing linters that come from the project, or whose config can run code, run only in projects you trust. That covers ESLint, Biome, Prettier, oxfmt, dprint, Ruff and Black, plus the `--verify` checks. rustfmt and gofmt need trust too: they come from your system, but project files can choose which one runs, as `rust-toolchain.toml` does for rustup.

- `cuilo add` asks once per project and remembers your answer.
- `cuilo trust` trusts the project without asking. `cuilo trust --revoke` stops trusting it and remembers that answer. Runs with `--no-input`, `--json` and MCP sessions never ask, so they rely on this command.
- Answers are saved in `trusted-projects.toml` next to your user config, by the project's full path. They live outside the project, so a cloned project cannot trust itself.
- `[tools] trust = "always"` or `"never"` in your [user config](/reference/user-config/) skips the question everywhere.
- The question names only formatters cuilo knows, so it shows no text the project wrote.
- Until you trust a project, plans say which files were not formatted.
- Revoking trust also reaches `cuilo mcp` sessions that are already running: on their next call, they stop running the project's tools.

The [formatters and trust guide](/guides/formatters-and-trust/) has the details.

## Signatures and signer pinning

A pack can be signed with [minisign](https://jedisct1.github.io/minisign/). The signature covers the one-line message `cuilo pack <name> <version> <digest>` and lives in `pack.minisig` at the pack root. You choose which keys to trust in the `[signing]` table of your user config:

```toml
[signing]
require = true                 # refuse unsigned packs and packs signed by keys you have not trusted
keys = { acme = "RWQ…" }       # minisign public keys
revoked = ["1C3B7AE36BE79191"] # key ids refused even when the signature is valid
```

- These are always refused: a `pack.minisig` that is not a minisign signature, a signature by a revoked key, and a signature by a trusted key that does not match the pack's files. Revoked keys come from your `revoked` list and from a list built into cuilo.
- An unsigned pack, or one signed by a key you have not trusted, loads only when `require` is off. It then counts as unsigned.
- Two trusted keys with the same key id are a config error.
- The lockfile records the whole public key that verified each pack, not only its key id. A later version signed by another key, or not signed, is refused.
- Official packs ship inside cuilo and need no signature.

See [Sign a pack](/packs/signing/).

## Lockfile digests

`cuilo.lock` records each pack's source and the SHA-256 digest of its files. The signature file is not part of the digest.

- A pinned pack loads from the cache, offline.
- A source that now serves different files for the same pin is refused.
- A pack whose files differ from the pinned ones for the same version is refused.
- A lockfile record loads only the pack and version it names.

Official packs are the exception: they ship with cuilo, so a newer cuilo brings their next version. See the [lockfile reference](/reference/lockfile/).

## Reserved official names

The official pack names, `ui` and `backend`, are reserved. A pack from any other source that uses one of them is refused, so a download cannot stand in for an official pack.

## Downloads and credentials

- Downloads use HTTPS only and are checked against the operating system's trusted certificates. A download follows at most 5 redirects, and only to HTTPS.
- Archives are read in memory and never unpacked onto disk. Only regular files are read, so links inside an archive are skipped. An archive may hold up to 20,000 entries and 64 MB of files.
- Credentials come from your user config, never from the project config, so a cloned project cannot send your tokens elsewhere. The config stores only the name of the environment variable that holds the token.
- A token goes only to its own host, never to a host a download redirects to.
- `git:` sources work over SSH only and run your own `git`. cuilo never prompts (SSH runs in batch mode unless you set your own SSH command), stops git after two minutes and uses `git archive`, so nothing is checked out.

See [Pack sources](/packs/sources/).

## Undo and a planted journal

The journal in `.cuilo/history` is project state, and a cloned project can carry one. `cuilo undo` checks the last entry before it writes anything:

- It refuses an entry that names `.git`, `.cuilo` or `cuilo.lock`, in any letter case or under another name for the same entry. cuilo never writes those paths, so it did not record that entry. `--force` does not override this. The error tells you to delete the entry from `.cuilo/history` if you did not create it.
- It refuses when a file holds neither what the transaction left there nor, for an interrupted transaction, what it found. `--force` overrides this, and the edits made since are lost.

See the [history guide](/guides/history/).

## Terminal output

Text from packs, projects and other programs is escaped before cuilo prints it. Control characters other than line feeds and tabs, and the characters that reorder text (U+202A to U+202E and U+2066 to U+2069), are shown as escapes such as `\u{1b}`. A pack or project cannot move the cursor, clear lines or rewrite what your terminal shows. Text that must stay on one line also shows line feeds as `\n`.

## Install integrity

Every channel installs the same binaries, built and attested by GitHub Actions:

| Channel                       | What checks it                                          |
| ----------------------------- | ------------------------------------------------------- |
| npm, pnpm, bun, yarn          | Lockfile integrity, npm provenance                      |
| Install script (macOS, Linux) | SHA-256, and the signature when `minisign` is installed |
| Install script (Windows)      | SHA-256, and the signature when `minisign` is installed |
| Homebrew                      | The formula's SHA-256                                   |
| cargo                         | The signature (`cargo binstall`), crates.io checksums   |

- The npm package has no dependencies and runs nothing at install time.
- The install scripts download over HTTPS only and follow no redirects. `install.sh` also requires TLS 1.2 or newer. `CUILO_REQUIRE_SIGNATURE=1` makes them refuse to install unless `minisign` checks the signature.
- `cuilo upgrade` and automatic updates replace only copies the install script made. They replace the binary only when the release's `SHA256SUMS` carries a valid signature by the release key built into cuilo, and the binary matches its signed checksum. Automatic updates never move to an older release, and `cuilo upgrade` does only with `--version`.
- For other global installs, `cuilo upgrade` runs the package manager that installed them, as a program with fixed arguments and never through a shell, and that package manager checks the download. It runs nothing for a copy from a one-off runner such as `npx`, or one in a project's `node_modules`. Automatic updates never run a package manager.
- The package manager runs in your cuilo cache folder, or your home folder when there is none, never in the folder you run `cuilo upgrade` from. So a cloned project's config, such as Yarn's `yarnPath`, a Cargo alias or `rust-toolchain.toml`, cannot steer it. On Windows, cuilo starts only a program it finds in an absolute `PATH` folder, never one in the current folder or next to cuilo.
- `cargo binstall` never installs QuickInstall's builds, which the release key did not sign. cuilo's crate turns that fallback off, and `cuilo upgrade` also passes `--disable-strategies quick-install`.
- The daily update check only reads which version is newest. `updates = "off"` in your user config, or `CUILO_UPDATES=off`, turns it off.
- Each signature names its version and file, so a signature cannot be reused for another file or version.
- The file that names the newest release is not signed, so a stable cuilo refuses a pre-release named there. Only `--version` moves a stable copy to a pre-release.
- To see where a binary was built: `gh attestation verify $(command -v cuilo) --repo TuentyFaiv/cuilo`.

Every release file is signed with this minisign key:

```text
RWTFcgVmt+E7Xi77vi6Q/Yu68TJDTsBTKYHqOmyjSOY4VCgpEFCwl+dK
```

:::note[The limit]
The checksums come from the same host as the binaries, so on their own they only catch a corrupted download. The signature and the attestation are what show a release came from cuilo's release workflow. The first `curl … | sh` trusts the download host over HTTPS, as rustup, Bun and Deno do. For more, download `install.sh` and `install.sh.minisig`, check them with `minisign -V -P <key> -m install.sh`, and run the script with `CUILO_REQUIRE_SIGNATURE=1`. Windows and macOS binaries are not code-signed with Authenticode or Apple notarization yet.
:::

See [Install](/install/).

## MCP tool annotations

Every MCP tool carries annotations that tell the client what it does. Read-only tools have `readOnlyHint: true`. `apply_plan` and `undo` write the project, and `verify_plan` runs code from the pack, so all three have `readOnlyHint: false` and `destructiveHint: true`. `verify_plan` changes nothing in the project, but it is marked this way so clients ask you before it runs. `add_pack` and `plan_update` have `openWorldHint: true` because they can download packs.

Annotations and the server's instructions guide agents and clients. They cannot stop an agent that ignores them, so keep your client's approval prompts on. See [Connect an agent over MCP](/agents/mcp/).

## Report a vulnerability

Report vulnerabilities privately through GitHub's advisory form: [github.com/TuentyFaiv/cuilo/security/advisories/new](https://github.com/TuentyFaiv/cuilo/security/advisories/new). Do not open a public issue. Include the cuilo version (`cuilo --version`), your operating system, and the steps or the pack that shows the problem.
