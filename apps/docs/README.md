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
