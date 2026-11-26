---

title: Packs and pieces
description: The official packs and their pieces, and how to name a piece, set its inputs, pick a variant and choose where its files go.
---

A pack is a versioned set of pieces. A piece is one thing you can generate, such as a component or a REST resource. This guide covers the official packs and the options every piece shares.

## The official packs

`cuilo pack list` shows the packs you can use in the current project:

```sh
cuilo pack list
```

```text
backend 0.1.0  official:backend
  database, error, middleware, resource
ui 0.2.0  official:ui
  atom, molecule, organism
```

The official packs ship inside the cuilo binary, so a newer cuilo brings their next version. Their names, `ui` and `backend`, are reserved. A pack from any other source must use another name.

| Piece                | What it generates                                                         | Default folder          |
| -------------------- | ------------------------------------------------------------------------- | ----------------------- |
| `ui/atom`            | The smallest UI building block, such as a badge, icon or label            | `src/ui/atoms`          |
| `ui/molecule`        | A small group of atoms with one job, such as a labelled field             | `src/ui/molecules`      |
| `ui/organism`        | A distinct section of an interface built from molecules, such as a header | `src/ui/organisms`      |
| `backend/resource`   | A REST resource in layers: model, repository, service, routes and tests   | `src/modules/<plural>`  |
| `backend/error`      | Application errors rendered as RFC 9457 problem details                   | `src/shared`            |
| `backend/middleware` | Middleware that propagates or creates a request header                    | `src/shared/middleware` |
| `backend/database`   | A SQLAlchemy async engine, sessions and declarative base for FastAPI      | `app`                   |

Some variants use their own folder. For example, Flutter writes to `lib/`, Axum to `src/` and the Go variants to `internal/`. The [ui](/packs/ui/) and [backend](/packs/backend/) pages list every stack and folder.

## Name a piece

Write a piece as `pack/piece`. If only one loaded pack has a piece with that name, you can leave out the pack:

```sh
cuilo add atom status-badge
```

If several packs have it, cuilo stops and tells you to write it as `pack/piece`.

## Set inputs

`cuilo pack show <pack>` lists each piece with its inputs and variants:

```sh
cuilo pack show backend
```

```text
backend 0.1.0
Layered backend pieces: REST resources, problem-details errors and middleware for Hono, Express, Axum, FastAPI, chi and Echo.

backend/database  SQLAlchemy async engine, sessions and the declarative base for FastAPI.
  variants: fastapi

backend/error  Application errors rendered as RFC 9457 problem details, with the framework's error handler.
  -i tests=…  Add tests; on by default when the project can run them.
  variants: hono, express, axum, fastapi, chi, echo

backend/middleware  Middleware that propagates or creates a request header, e.g. `request-id`, and exposes it to handlers.
  -i name=…  Header name without `x-`, e.g. `request-id` or `correlation-id`.
  -i tests=…  Add tests; on by default when the project can run them.
  variants: hono, express, axum, fastapi, chi, echo

backend/resource  A REST resource in layers: model, repository, service, routes and tests; adds the `error` piece if missing.
  -i module=…  Go module path from `go.mod`.
  -i name=…  Singular name, e.g. `blog-post`.
  -i plural=…  Plural name for routes, tables and folders, e.g. `blog-posts`.
  -i tests=…  Add tests; on by default when the project can run them.
  variants: hono, express, axum, fastapi, chi, echo
```

The argument after the piece fills its `name` input. You can write it in any case form, so `blog-post` and `BlogPost` give the same files. Set the other inputs with `-i key=value`, once per input. True-or-false inputs accept `true` or `false`, `yes` or `no`, `y` or `n`, `1` or `0`, and `on` or `off`.

In a Hono project with TypeScript and Vitest:

```sh
cuilo add backend/resource blog-post -i plural=blog-posts --dry-run
```

```text
add backend/resource (with error)
  create  src/shared/error-handler.ts
  create  src/shared/errors.test.ts
  create  src/shared/errors.ts
  create  src/modules/blog-posts/blog-post.model.ts
  create  src/modules/blog-posts/blog-post.repository.ts
  create  src/modules/blog-posts/blog-post.routes.test.ts
  create  src/modules/blog-posts/blog-post.routes.ts
  create  src/modules/blog-posts/blog-post.service.test.ts
  create  src/modules/blog-posts/blog-post.service.ts
Packages to add (offered after applying; --install adds them without asking):
  npm      zod ^4.1
  npm      vitest ^4.0 (dev)
```

The `(with error)` part means `resource` builds on the `error` piece. cuilo adds `error` first when the project does not have it yet, and passes on the inputs they share, such as `tests`.

If you leave out an input that has no default, cuilo asks for it in a terminal:

```text
Plural name for routes, tables and folders, e.g. `blog-posts`:
```

With `--no-input`, or without a terminal, it stops instead:

```text
error: input `plural` is required
```

cuilo never asks about true-or-false inputs. They take their default, and some defaults depend on the stack. For example, `tests` is on when the project can run the tests, such as a TypeScript project with Vitest.

## Choose a variant

Each piece picks a variant from the [detected stack](/guides/stack-detection/). To use another one, pass its name from `cuilo pack show`:

```sh
cuilo add ui/atom status-badge --variant svelte --dry-run
```

```text
add ui/atom
  create  src/ui/atoms/status-badge/status-badge.svelte
  create  src/ui/atoms/status-badge/index.ts
```

## Choose where files go

cuilo picks the folder in this order:

1. `--target` on the command line.
2. `paths` in `cuilo.json`.
3. The variant's own folder, if it has one.
4. The piece's default folder.

```sh
cuilo add ui/atom status-badge --target src/ui/badges --dry-run
```

```text
add ui/atom
  create  src/ui/badges/status-badge/status-badge.module.css
  create  src/ui/badges/status-badge/status-badge.tsx
  create  src/ui/badges/status-badge/index.ts
```

To set a folder for every run, add `paths` to `cuilo.json`. Use `defaults` to set input values. The keys are piece names without the pack:

```json title="cuilo.json"
{
	"paths": { "atom": "src/ui/primitives" },
	"defaults": { "atom": { "tests": true } }
}
```

```sh
cuilo add ui/atom status-badge --dry-run
```

```text
add ui/atom
  create  src/ui/primitives/status-badge/status-badge.module.css
  create  src/ui/primitives/status-badge/status-badge.test.tsx
  create  src/ui/primitives/status-badge/status-badge.tsx
  create  src/ui/primitives/status-badge/index.ts
Packages to add (offered after applying; --install adds them without asking):
  npm      vitest ^4.0 (dev)
  npm      @testing-library/react ^16.3 (dev)
  npm      @testing-library/dom ^10.4 (dev)
```

A value you pass with `-i` wins over `defaults`. See the [`cuilo.json` reference](/reference/configuration/).

:::caution
A pack can write only to the paths its `pack.toml` allows. The `ui` pack allows `src/ui/**` and the folders its Flutter, Compose and SwiftUI variants use. A folder outside them rejects the plan with exit code 3:

```text
The plan was rejected:
  src/components/status-badge/status-badge.module.css: the pack does not declare permission to write here
  src/components/status-badge/status-badge.tsx: the pack does not declare permission to write here
  src/components/status-badge/index.ts: the pack does not declare permission to write here
```
:::

## Use other packs

`--from <source>` loads a pack before generating from it:

```sh
cuilo add mine/note release-plan --from ./packs/mine
```

Once you apply that plan, `cuilo.lock` pins the pack. After that, `cuilo pack list` shows it, and later commands find it by name without `--from`.

`cuilo pack add <source>` loads and verifies a pack without generating anything:

```sh
cuilo pack add ./packs/mine
```

```text
Loaded mine 0.1.0 from path:./packs/mine (not signed by a trusted key)
```

A source can be a folder, a GitHub tag (`gh:`), a git repository over SSH (`git:`), an HTTPS tarball, a registry item (`registry:`) or a namespace from `cuilo.json`. See [Pack sources](/packs/sources/), [Signing](/packs/signing/) and [Write a pack](/packs/write-a-pack/).
