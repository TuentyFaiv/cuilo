---

title: Changelog
description: What is new in each cuilo version, starting with 2.0.0-alpha.0, the rewrite that replaces tfverse.
---

## 2.0.0-alpha.0 (2026-11-27)

The first version of cuilo 2. It is a rewrite of tfverse, the v1 CLI, under a new name: the command is `cuilo` and the npm package is `@cuilo/cli`. See [Migrating from v1](/migrating-from-v1/).

### Packs and pieces

- Code comes from packs: versioned bundles with a `pack.toml` and one folder per piece. See [Write a pack](/packs/write-a-pack/).
- Templates are real code. Each input names an example, and cuilo replaces it in every case form, file names included. Optional code uses `@cuilo if`, `@cuilo else` and `@cuilo end` comment directives.
- Pieces declare typed inputs once. The same definition drives terminal prompts, `-i key=value` flags and what `describe_piece` tells agents.
- Pieces adapt to the detected stack through variants and stack conditions such as `default_when`. `cuilo detect` shows what was found.
- Echo and Solid also get their major version, such as `echo-v5` or `solid-v2`, from `go.mod` and `package.json`, so a variant can target one major. When a variant fails only for lack of the major it needs, cuilo says which one, as for Solid 1, since the `ui` pack supports Solid 2 only. The `backend` pack supports Echo v4 and v5. See [Stack detection](/guides/stack-detection/#how-variants-pick-the-code).
- Generated JavaScript and TypeScript write relative imports the way the project resolves them: `./foo` for bundlers, `./foo.js` under `NodeNext` or `Node16`, and `./foo.ts` in Deno or with `allowImportingTsExtensions` or `rewriteRelativeImportExtensions`. cuilo reads the `tsconfig.json` that governs each piece's folder, with what it extends, and records the style as the `import_style` fact, which `cuilo detect` shows, `cuilo.json` can set and `cuilo.lock` keeps for updates. It reads those configs through symlinks and Windows junctions, such as pnpm's links in `node_modules`, only while every link stays inside the project, and never looks at a path outside it. Pack templates keep extensionless imports. See [relative imports in generated code](/guides/stack-detection/#relative-imports-in-generated-code).
- A piece can require other pieces of its pack, list the packages its code imports, and patch existing files with `anchor`, `import` and `css_vars` patches. A patch's `file` can list alternatives, and the first one the project has is used; an `anchor` can too, and the first one the file contains is used. A variant's `target` can list alternatives as well, and the first folder the project has is used.
- Pieces list next steps after `cuilo add`, such as mounting new routes, instead of setup hints in the generated code. With `--wire`, or `wire = true` in the user config, cuilo does the steps it can as patches in the plan. See [next steps and wiring](/guides/packs-and-pieces/#next-steps-and-wiring).
- `kinds` in `pack.toml` says what a pack is for (`ui`, `backend`, `mobile` or `desktop`), and `[translations.<locale>]` gives the descriptions of the pack, its pieces and their inputs in other languages. The pack directory filters by kind and shows the descriptions in the page's language. See [descriptions in other languages](/packs/write-a-pack/#descriptions-in-other-languages).
- Pack sources: official packs built into the binary, folders, `gh:` tags or commits, HTTPS tarballs, `git:` over SSH, `registry:` items and namespaces from the project config. See [Pack sources](/packs/sources/).
- Two official packs ship with cuilo: `ui` 0.2.0 (`atom`, `molecule`, `organism`) and `backend` 0.2.0 (`resource`, `error`, `middleware`, `database`).
- `cuilo adopt` takes over existing code as a piece. `cuilo pack capture` turns project files into a piece of a local pack. See [Adopt and capture](/guides/adopt-and-capture/).
- `cuilo pack list`, `cuilo pack show` and `cuilo pack add` list, describe and load packs.

### Plans and history

- Every change is a plan first. `--dry-run` shows it, `--diff` adds diffs, and `--review` keeps or skips each hunk, like `git add -p`. Nothing is written until you apply.
- Every apply is a transaction journaled in `.cuilo/history`. `cuilo log` lists them, and `cuilo undo` reverts the last one, refusing to drop edits made since unless `--force`. See [History and undo](/guides/history/).
- `cuilo.lock` records the packs and generated files. `cuilo status` shows whether each file is clean, modified or missing, and `cuilo why <path:line>` tells whether a line came from the pack or from you.
- Generated files go through the project's own formatter and fixing linters (ESLint, Biome, Prettier, oxfmt, dprint, Ruff, Black, rustfmt, gofmt) while planning, in trusted projects. See [Formatters and trust](/guides/formatters-and-trust/).
- Plans list the packages their pieces need. After applying, cuilo offers the command for the project's package manager, and `--install` or `--no-install` decide without asking. See [Packages a piece needs](/guides/dependencies/).
- `--verify` runs the project's own checks on a throwaway copy with the plan applied, and refuses the plan when a check fails because of it. See [Verify a plan](/guides/verify/).

### Updates

- `cuilo update <pack>` moves every piece from a pack to another version as one plan, merging your edits line by line.
- `--on-conflict` chooses what happens where your edits and the new version change the same lines: `refuse` (the default), `markers`, `ours` or `theirs`.
- Packs can ship migrations for code they do not own, as a built-in whole-word `rename` or as ast-grep rules. `--no-migrate` skips them.
- A hunk you skip in `--review` counts as your edit, so later updates keep it skipped.

See [Update generated code](/guides/updates/).

### Safety

- A pack writes only the paths its `permissions.write` globs declare, and never `.git`, `.cuilo` or `cuilo.lock`, in any letter case or under another name for the same entry.
- Plans flag risky paths, such as `package.json`, CI workflows, editor settings and agent instructions.
- `--verify` starts the checks only after you see the plan and agree, or with `--yes`, and only in a project you trust. Copies live in a private folder of your cache.
- Project trust: the project's own tools run only after you trust it with the question `cuilo add` asks or with `cuilo trust`. `cuilo trust --revoke` takes trust back, also for running MCP sessions.
- Packs can be signed with minisign. You can require signatures, trust and revoke keys, and the lockfile pins the key that signed each pack.
- The lockfile pins each pack by the SHA-256 of its files. A source that serves different files for the same pin is refused.
- The official names `ui` and `backend` are reserved for the official packs.
- `cuilo undo` refuses a journal entry that names a path cuilo never writes, so a cloned project cannot plant one.
- Text from packs and projects is escaped before it reaches the terminal.
- Credentials come only from your user config and go only to their own host.

See [Security](/security/).

### Agents

- `cuilo mcp` serves the plan, review and apply flow over MCP on stdio, with 18 tools. Nothing is written before `apply_plan`.
- Tool annotations mark `apply_plan`, `undo` and `verify_plan` as not read-only, so clients can ask you first.
- Every command takes `--json` and `--no-input`. Exit codes are `0` done, `1` failed, `2` usage or confirmation needed, and `3` plan rejected. See [Exit codes](/reference/exit-codes/).
- The docs site serves `/llms.txt` and `/llms-full.txt`. See [Give an agent the docs](/agents/llms-txt/).

See [Connect an agent over MCP](/agents/mcp/).

### Distribution

These channels open with the first release.

- One set of binaries for every channel, built and attested by GitHub Actions and signed with minisign.
- npm (`@cuilo/cli`) with no dependencies and nothing run at install time; the package manager picks the platform package.
- Install scripts for macOS, Linux and Windows that check the SHA-256, and the signature when `minisign` is installed. `CUILO_REQUIRE_SIGNATURE=1` makes the signature required.
- Homebrew (`tuentyfaiv/tap/cuilo`) and cargo (`cargo binstall cuilo`, `cargo install cuilo --locked`).
- `cuilo upgrade` replaces a script-installed copy with a newer signed release, and runs npm, pnpm, Bun, Yarn, Homebrew or cargo for copies they installed globally. For a copy from `npx`, `bunx`, `pnpm dlx` or `yarn dlx`, or one in a project's `node_modules`, it runs nothing and prints the command to run instead. `cuilo upgrade --check` only reports what is available.
- In a terminal, cuilo looks for a new release once a day in the background. Script-installed copies update themselves, other copies print a notice, and copies from one-off runners do neither. `updates` in the user config, or `CUILO_UPDATES`, picks `auto`, `notify` or `off`; cuilo warns about any other `CUILO_UPDATES` value and ignores it.
- `cuilo completions <bash|zsh|fish|powershell|elvish>` prints a shell completion script.

See [Install](/install/).

### Known limits

- Nothing is published yet. Build cuilo from source until the first release opens the install channels.
- No command creates a new project. cuilo adds code to an existing one.
- Many v1 kinds, such as pages, layouts, hooks and stores, have no official piece. See [Migrating from v1](/migrating-from-v1/).
- `registry:` items do not apply their `css`, `tailwind` and `envVars` fields yet. The pack description names them.
- `git:` sources work over SSH only.
- Changes a package manager makes are outside cuilo's history, so `cuilo undo` does not revert them.
- The `--verify` copy is not a sandbox: the checks run as you. It also refuses projects with more than 50,000 files or 1 GB outside build and dependency folders.
- Pending MCP plans and packs loaded with `add_pack` last only for the session.
- Windows and macOS binaries are not code-signed with Authenticode or Apple notarization.
