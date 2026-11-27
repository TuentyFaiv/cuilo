# cuilo.dev docs

Astro + Starlight, in English and Spanish (`src/content/docs/es-mx/`), served as static files from Cloudflare
Workers.

```bash
bun run docs           # from the repository root: dev server
bun run docs:build     # type check and build into apps/docs/dist
cargo xtask docs       # regenerate the CLI and schema reference and src/data/packs.json
```

The CLI reference, the file references and `src/data/packs.json` are generated; CI fails when they differ from the
code (`cargo xtask docs --check`).

## Navigation

The header tabs come from the sections in `src/sections.ts`, keyed by path prefix. The sidebar groups live in
`astro.config.mjs`, and `src/route-data.ts` shows each page only its own section's groups, with Previous and Next
links that stay inside the section. When a page moves, add its old URL to `public/_redirects`, with and without the
trailing slash.

## Pack directory

`src/components/packs/` draws the directory and the pack pages from `src/data/packs.json`. The generator copies each
pack's `kinds` and every description's translations from the manifests, keeps each piece's inputs in its manifest's
order, and records an example per variant: the files `cuilo add --dry-run` plans for it in an empty project. The pack
page shows the folder of the example whose variant comes first in priority as the piece's output folder. Pages in
another locale show the manifests' translations for it, and English where a pack has none; `llms.txt` stays English.
`src/packs.ts` derives the rest, such as official packs being Built in. The directory keeps its search, filters and
sort in the URL's query, and a pack page keeps its tab in the hash (`#tab-stacks`).

`src/stack-priority.ts` is the one order the site lists stacks in, in the finder, the directory, the pack pages and
`llms.txt`: Svelte, Astro, the Rust stacks (Leptos, Dioxus, Yew, Axum) and SwiftUI first, then the other UI stacks,
then the other backend stacks. The finder lists pieces by the same order, so it starts on a Svelte piece. Variants
for one stack with one ORM, such as `hono-drizzle`, count as that stack (Hono) in chips, filters and counts, and show
their own name (`Hono · Drizzle`) where the site names the variant.

The site has no native `<select>`: `src/components/select-menu.astro` is the one select the finder, the directory's
sort and the language and theme pickers share.

## Controls

Inputs, select triggers and button-like actions take their size, radius, border, background and focus ring from the
`--cu-control-*` tokens and the `.cu-control` classes in `src/styles/theme.css`. There are two sizes: the default
(2.5rem) for fields, toolbars, the mobile menu and the hero's actions, and `cu-control--compact` (2rem) for the header
and for actions inside cards and lists. Controls in one row use the same size. Text fields and the selects beside them
use 1rem type, which keeps iOS Safari from zooming in on focus. Icon-only links and pickers, such as the header's
GitHub and theme icons, are `cu-control--ghost`: no border or fill until hovered.

## Deployment

[Workers Builds](https://developers.cloudflare.com/workers/ci-cd/builds/) builds and deploys the site from GitHub: a
push to `main` deploys `cuilo.dev`, and a push to any other branch builds a private preview. Nothing is deployed by
hand.

The settings live in the Cloudflare dashboard (**Workers & Pages** → `cuilo` → **Settings** → **Builds**):

| Setting           | Production                                                                               | Previews                      |
| ----------------- | ---------------------------------------------------------------------------------------- | ----------------------------- |
| Branch            | `main`                                                                                   | every other branch            |
| Root directory    | `/` (the repository root, where `bun.lock` and `bunfig.toml` are)                        | `/`                           |
| Build command     | `bun run docs:build`                                                                     | `bun run docs:build`          |
| Deploy command    | `bun run docs:deploy`                                                                    | `bun run docs:deploy-preview` |
| Build watch paths | Include `apps/docs/*`, `package.json`, `bun.lock`, `bunfig.toml`, `npm/cli/package.json` | the same                      |

Workers Builds runs `bun install --frozen-lockfile` before the build command, with the Bun version from
`packageManager` in the root `package.json`.

`wrangler.jsonc` attaches the `cuilo.dev` custom domain on every production deploy. Other branches deploy as
[Worker Previews](https://developers.cloudflare.com/workers/previews/) on `workers.dev`, which stay private through
Cloudflare Access: on the Worker, enable Access for preview URLs only, with a policy that allows only the maintainer's
email.
