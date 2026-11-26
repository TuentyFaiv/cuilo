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

| Setting           | Value                                                                                    |
| ----------------- | ---------------------------------------------------------------------------------------- |
| Repository        | `TuentyFaiv/cuilo`                                                                       |
| Production branch | `main`                                                                                   |
| Root directory    | `/` (the repository root, where `bun.lock` and `bunfig.toml` are)                        |
| Build command     | `bun install --frozen-lockfile && bun run docs:build`                                    |
| Deploy command    | `bun run docs:deploy`                                                                    |
| Preview command   | `bun run docs:deploy-preview`                                                            |
| Build variables   | `BUN_VERSION` = `1.4.4` (the build image ships an older Bun)                             |
| Build watch paths | Include `apps/docs/*`, `package.json`, `bun.lock`, `bunfig.toml`, `npm/cli/package.json` |

`wrangler.jsonc` attaches the `cuilo.dev` custom domain on every deploy and turns on `workers.dev` preview URLs.
Preview URLs stay private through Cloudflare Access (**Settings** → **Domains & Routes** → **Preview URLs** →
**Enable Cloudflare Access**, with a policy that allows only the maintainer's email).
