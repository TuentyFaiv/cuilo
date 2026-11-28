---

title: Introduction
description: What cuilo does, the ideas behind it (packs, pieces, variants, plans, the lockfile and history) and where to start.
---

cuilo generates code into your project from versioned **packs**: components, REST resources, middleware and other pieces of an app. You see every change as a plan before anything is written. The code it adds is yours to edit, and when the pack releases a new version, `cuilo update` merges the changes into your edited files.

:::note
cuilo 2 is in alpha: [install it](/install/) to try it, and expect breaking changes until the beta.
:::

## Packs and pieces

A **pack** is a versioned folder of pieces with a `pack.toml` that names it, gives its version and lists the paths it may write. Two official packs ship inside the cuilo binary:

- `ui`: atomic design components (`atom`, `molecule`, `organism`) for Svelte, Astro, Leptos, Dioxus, Yew, SwiftUI, Angular, Qwik, Solid, Vue, React, Lit, Flutter and Jetpack Compose (Android and Multiplatform).
- `backend`: a layered REST `resource`, RFC 9457 `error` responses, request header `middleware` and a `database` setup, for Axum, Hono, Express, FastAPI, chi and Echo.

A **piece** is one thing a pack can generate. You name it as `pack/piece`:

```sh
cuilo add ui/atom status-badge
```

Packs can also come from a folder, a git tag, an HTTPS tarball or a registry item. See [Pack sources](/packs/sources/).

## Inputs

Each piece declares typed **inputs**: text, true or false, a choice from a list, or a number. The `name` argument fills the `name` input, and `-i key=value` sets the others. In a terminal, cuilo asks for the inputs you left out that have no default. With `--no-input`, it fails instead. The same definition also gives AI agents a schema over MCP.

## Variants

Pieces adapt to your stack through **variants**. cuilo reads your project files to learn its framework, language, styling, test library and more ([`cuilo detect`](/reference/cli/detect/) shows the result). Each variant names the facts it needs, and cuilo uses the first one that matches. A Svelte and TypeScript project gets a `.svelte` component. A Leptos project gets a Rust component in `src/ui/atoms/`. If you use Tailwind, the component gets utility classes instead of its own styles. See [Stack detection](/guides/stack-detection/).

## The plan

Commands that change files build a **plan** first. It lists every file to create, modify, replace or delete, the packages the code imports, and notes. Risky paths such as `package.json` or CI workflows are flagged. A pack can write only to the paths its `pack.toml` allows, and any other path rejects the plan.

You choose what happens next:

- `--dry-run` shows the plan and stops.
- `--diff` adds the diff of each changed file.
- `--review` lets you keep or skip each hunk, like `git add -p`.
- `--verify` runs the project's own checks on a throwaway copy first. See [Verify a plan](/guides/verify/).
- `--yes` applies without asking.

## History and undo

Applying a plan writes one **transaction**. cuilo keeps its history in `.cuilo/` at the project root, a folder that ignores itself in Git. [`cuilo log`](/reference/cli/log/) lists transactions, and [`cuilo undo`](/reference/cli/undo/) reverts the last one. Undo refuses when a file changed after the transaction, so your later edits are not lost by accident. See [History and undo](/guides/history/).

## The lockfile

`cuilo.lock` is the file you commit. It pins each pack by its source and the SHA-256 of its files, so a source that later serves different files for the same pin is refused. For each generated piece, it records the variant, target folder, detected stack, inputs, and the hash of every file as cuilo wrote it.

That record lets cuilo tell your code from the pack's:

- [`cuilo status`](/reference/cli/status/) shows each generated file as `clean`, `modified` or `missing`.
- [`cuilo why path:line`](/reference/cli/why/) says whether a line came from the pack or from you.

See the [lockfile reference](/reference/lockfile/).

## Updates that merge

[`cuilo update <pack>`](/reference/cli/update/) moves every piece from a pack to a new version as one plan. Files you never touched take the new version. Files you edited are merged three ways, line by line, using the original cuilo wrote as the base. When you and the pack change the same lines, the default is to refuse the plan. You can also write Git-style markers, keep your lines, or take theirs. See [Updating packs](/guides/updates/).

## Who it is for

- Developers who create the same kinds of files again and again and want them to fit the stack they already use.
- Teams that keep their conventions in a pack and want fixes to reach code that was already generated. See [Write a pack](/packs/write-a-pack/).
- AI agents. Every command takes `--json` and `--no-input`, and [`cuilo mcp`](/agents/mcp/) serves the same plan, review and apply steps over MCP.

## Where to go next

- [Install cuilo](/install/).
- [Generate your first piece](/first-piece/) in a SvelteKit project, then edit it, inspect it and undo it.
- [Packs and pieces](/guides/packs-and-pieces/) covers inputs, variants and output folders.
- [Security](/security/) explains what packs can and cannot do.
