---

title: Stack detection
description: How cuilo reads your project files to learn its stack, how the stack picks the code each piece generates, and how to override it.
---

Before it plans a piece, cuilo reads your project's manifest files and builds a list of stack facts. Pieces use these facts to choose a variant, and to keep or leave out files and lines.

## See what cuilo detects

Take a Next.js project with this `package.json`, and a `.env.example` that defines `DATABASE_URL` and `API_TOKEN`:

```json title="package.json"
{
	"packageManager": "bun@1.3.0",
	"dependencies": { "next": "16.0.0", "react": "19.2.0", "drizzle-orm": "1.0.0" },
	"devDependencies": { "typescript": "6.0.0", "tailwindcss": "4.1.0", "vitest": "4.0.0", "@biomejs/biome": "2.3.0" }
}
```

```sh
cuilo detect
```

```text
{
  "stack": {
    "framework": [
      "next",
      "react"
    ],
    "language": [
      "typescript"
    ],
    "runtime": [
      "bun"
    ],
    "database": [
      "drizzle"
    ],
    "styling": [
      "tailwind"
    ],
    "package_manager": [
      "bun"
    ],
    "testing": [
      "vitest"
    ],
    "formatter": [
      "biome"
    ],
    "import_style": [
      "imports-extensionless"
    ]
  },
  "env": [
    "API_TOKEN",
    "DATABASE_URL"
  ]
}
```

`cuilo detect` always prints JSON. It only reads files and runs nothing.

## The facts

| Fact              | What it holds                                                                   | Examples                                             |
| ----------------- | ------------------------------------------------------------------------------- | ---------------------------------------------------- |
| `framework`       | Frameworks and UI libraries                                                     | `react`, `next`, `svelte`, `hono`, `axum`, `flutter` |
| `language`        | Languages                                                                       | `typescript`, `javascript`, `rust`, `python`, `go`   |
| `runtime`         | The JavaScript runtime, or Tokio for Rust                                       | `node`, `bun`, `deno`, `tokio`                       |
| `database`        | The ORM, ODM or query builder that database code follows                        | `drizzle`, `prisma`, `sqlx`, `sqlalchemy`, `gorm`    |
| `styling`         | CSS tools                                                                       | `tailwind`, `sass`, `stylex`, `unocss`               |
| `package_manager` | Package managers                                                                | `npm`, `pnpm`, `bun`, `cargo`, `uv`, `gradle`        |
| `testing`         | Test libraries the project installed. Built-in runners are not listed.          | `vitest`, `jest`, `playwright`, `pytest`             |
| `formatter`       | Formatters and fixing linters                                                   | `biome`, `prettier`, `eslint`, `ruff`, `rustfmt`     |
| `import_style`    | How generated JavaScript and TypeScript write relative imports. Only one value. | `imports-extensionless`, `imports-js`, `imports-ts`  |

A fact can hold several values, as `framework` does for a Next.js app. Database drivers, SQL files and schema files do not count as a `database`. `env` lists the variable names in `.env.example`. cuilo never reads their values.

## Where the facts come from

cuilo reads files at the project root: the current folder, or the one you pass with `--root`. Only packages the project declares count. Packages that arrive as dependencies of other packages do not.

| Ecosystem                 | Files                                                                                                      |
| ------------------------- | ---------------------------------------------------------------------------------------------------------- |
| JavaScript and TypeScript | `package.json`, `deno.json` or `deno.jsonc` imports, `tsconfig.json` and the configs it extends, lockfiles |
| Rust                      | `Cargo.toml`, including dev, build and workspace dependencies                                              |
| Python                    | `pyproject.toml` (PEP 621, dependency groups and Poetry), `requirements.txt`, `Pipfile`, lockfiles         |
| Go                        | `go.mod`                                                                                                   |
| Dart and Flutter          | `pubspec.yaml`                                                                                             |
| Ruby                      | `Gemfile`                                                                                                  |
| PHP                       | `composer.json`                                                                                            |
| Kotlin and Java           | `build.gradle`, `build.gradle.kts`, `app/build.gradle(.kts)`, `gradle/libs.versions.toml`, `pom.xml`       |
| Swift                     | `Package.swift`, `Podfile`, an `.xcodeproj` or `.xcworkspace`                                              |

A few rules decide the less obvious facts:

- `language` is `typescript` when `package.json` lists `typescript`, the project has a `tsconfig.json`, or it is a Deno project. Otherwise it is `javascript`.
- The JavaScript package manager comes from `packageManager` in `package.json`. Without it, cuilo looks for `bun.lock`, `pnpm-lock.yaml`, `yarn.lock` and `package-lock.json`, in that order. It falls back to `deno` in a Deno project and to `npm` otherwise.
- The Python package manager is `uv` with a `uv.lock`, `poetry` with Poetry settings or a `poetry.lock`, `pdm` with a `pdm.lock`, `pipenv` with a `Pipfile`, and `pip` otherwise.
- Frameworks with their own ORM add it: Rails adds `activerecord`, Laravel adds `eloquent` and Django adds `django-orm`.
- Formatters come from dependencies and from config files such as `biome.json`, `.prettierrc`, `eslint.config.js`, `dprint.json`, `ruff.toml` or `rustfmt.toml`, in a project of any language. Rust projects also get `rustfmt`, and Go projects get `gofmt`.
- `import_style` comes from the `tsconfig.json` at the root, or Deno. Each piece then uses the one for its own folder. See [Relative imports in generated code](#relative-imports-in-generated-code).
- Other ecosystems, such as Elixir or .NET, get no facts.

## How variants pick the code

A piece lists its variants in order, each with the facts it needs. This part of the `ui/atom` manifest shows three of them:

```toml title="packs/ui/pieces/atom/piece.toml"
[[variants]]
name = "vue"
when = { framework = "vue", language = "typescript" }

[[variants]]
name = "react"
when = { framework = "react", language = "typescript" }

[[variants]]
name = "lit"
when = { framework = "lit", language = "typescript" }
```

cuilo uses the first variant whose facts all hold. The Next.js project above has `react` and `typescript`, so it gets the `react` variant.

Inside a variant, conditions on `stack.<value>` keep or drop files and lines. The atom leaves out its CSS module when `stack.tailwind` holds, and its `tests` input is on when `stack.vitest` holds. So in the Next.js project, the plan has a test file and no CSS module:

```sh
cuilo add ui/atom status-badge --dry-run
```

```text
add ui/atom
  create  src/ui/atoms/status-badge/status-badge.test.tsx
  create  src/ui/atoms/status-badge/status-badge.tsx
  create  src/ui/atoms/status-badge/index.ts
Packages to add (offered after applying; --install adds them without asking):
  npm      vitest ^4.0 (dev)
  npm      @testing-library/react ^16.3 (dev)
  npm      @testing-library/dom ^10.4 (dev)
Notes:
  cuilo.lock: not formatted with Biome: cuilo runs a project's formatters and linters only once you trust it; run `cuilo trust` here
```

The note is about running Biome on the generated files. See [Formatters and trust](/guides/formatters-and-trust/).

When no variant matches, cuilo lists the ones the piece has. A React project without TypeScript gets this, because the `react` variant needs `typescript`:

```text
error: no variant matches this project; available: angular, qwik, solid, astro, svelte, vue, react, lit, leptos, dioxus, yew, flutter, compose-multiplatform, compose, swiftui-package, swiftui
```

To choose a variant for one command, pass `--variant`. See [Packs and pieces](/guides/packs-and-pieces/#choose-a-variant).

`cuilo.lock` records the stack, variant and folder each piece was generated with. [`cuilo update`](/guides/updates/) renders the new pack version with those, so changing your stack later does not move a piece you already have to another variant.

## Relative imports in generated code

Packs write relative imports without an extension, such as `./blog-post.service`. That works wherever a bundler resolves files, as in Vite, Astro, SvelteKit, Next.js, Bun and Wrangler. TypeScript's `NodeNext` resolution needs the extension the file has once compiled, and Deno needs the file's own extension. So cuilo writes each relative import the way the project's own code must, and records that as the `import_style` fact:

| `import_style`          | An import of `blog-post.service.ts` | When                                                                                                                    |
| ----------------------- | ----------------------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| `imports-extensionless` | `./blog-post.service`               | `moduleResolution` is `bundler`, `node10`, `node` or `classic`, or there is no `tsconfig.json`                          |
| `imports-js`            | `./blog-post.service.js`            | `moduleResolution` is `nodenext` or `node16`, or it is unset and `module` is `node16`, `node18`, `node20` or `nodenext` |
| `imports-ts`            | `./blog-post.service.ts`            | `allowImportingTsExtensions` or `rewriteRelativeImportExtensions` is `true`, or the project uses Deno                   |

The file itself stays `blog-post.service.ts`. With `imports-js`, an import of a `.mts` file ends in `.mjs`, and one of a `.cts` file in `.cjs`. With `imports-ts`, each import ends in the extension of the file it names, such as `.tsx`. create-hono's Node.js and Vercel templates use `NodeNext`, so the `backend` pack writes `./blog-post.service.js` there, while its Bun and Cloudflare Workers templates keep `./blog-post.service`.

cuilo reads the `tsconfig.json` nearest to the folder a piece goes to, up to the project root, so a package in a monorepo can have its own style. It follows `extends`: relative paths, packages in `node_modules` (through their `exports`, their `tsconfig` field or their `tsconfig.json`), and lists of both, where later entries win. To read these configs and the `package.json` of each package, cuilo follows symlinks and Windows junctions, such as the ones pnpm makes in `node_modules`, but only while every link stays inside the project. cuilo follows each link itself, one folder at a time, and never looks at a path outside the project, so it does not follow a link that leaves the project, even one that comes back. It reads and writes every other project file without following links. A `deno.json` or `deno.jsonc` on the way up, or a `deno` runtime, means Deno. cuilo never reads a config outside the project, stops after 16 levels of `extends` or 256 steps in all (each file read, path looked up and folder searched for a package is one), and skips configs over 1 MiB and anything that is not a regular file, such as a FIFO. Configs it cannot read, and links that are broken, loop or lead outside the project, count as absent. `cuilo detect` shows the style at the project root.

How cuilo rewrites the imports:

- It looks for the file each relative import names among the files the plan writes and the files the project has. `./blog-post.service` names `blog-post.service.ts`, so a dot in a file name is not an extension. `./shared` names `shared/index.ts` when there is no `shared.ts`, and becomes `./shared/index.js`.
- It covers `import` and `export … from` statements, `import type`, `import "…"`, `export * from`, dynamic `import("…")` and, in CommonJS files, `require("…")`. It rewrites generated `.ts`, `.tsx`, `.mts`, `.cts`, `.js`, `.jsx`, `.mjs` and `.cjs` files, and the text that `import` and `anchor` patches add to such files, wiring included.
- Imports that already end in `.js`, `.ts` or another script extension stay as they are. So do imports of other files, such as `./styles.css` or `./card.svelte`, of packages, and of aliases such as `@/lib/utils`, `~/lib` or `#lib`. Comments, strings and lines with `@cuilo` directives are never changed.
- A relative import that names no file stays as written, and the plan notes it. For example, with the `error` piece moved to `src/errors` through `paths`:

  ```text
  Notes:
    src/modules/blog-posts/blog-post.routes.ts: no file matches the import `../../shared/errors`, so cuilo left it as the pack wrote it
  ```

- The rewrite runs before the project's formatter, so Biome or Prettier sees the final code.

`cuilo.lock` records the style with each piece's stack, and `cuilo update` keeps writing its imports that way, even after `tsconfig.json` changes, so the original it rebuilds for the merge matches what it wrote. Pieces recorded without a style keep extensionless imports. To choose the style yourself, set `import_style` in `cuilo.json`, as the next section shows.

## Override detection

Add `stack` to `cuilo.json` when detection gets your project wrong. Each fact you list replaces the detected values for that fact. The other facts are still detected:

```json title="cuilo.json"
{
	"stack": { "framework": ["svelte"] }
}
```

In the React project from [Your first piece](/first-piece/), `cuilo detect` then shows:

```text
{
  "stack": {
    "framework": [
      "svelte"
    ],
    "language": [
      "typescript"
    ],
    "runtime": [
      "node"
    ],
    "package_manager": [
      "npm"
    ],
    "import_style": [
      "imports-extensionless"
    ]
  },
  "env": []
}
```

And the atom comes out as a Svelte component:

```text
add ui/atom
  create  src/ui/atoms/status-badge/status-badge.svelte
  create  src/ui/atoms/status-badge/index.ts
```

`import_style` works the same way. With it set, cuilo reads no `tsconfig.json` to choose the style, and writes every piece's relative imports in the one you set:

```json title="cuilo.json"
{
	"stack": { "import_style": ["imports-js"] }
}
```

It takes one value: `imports-extensionless`, `imports-js` or `imports-ts`. Anything else stops cuilo with an error:

```text
error: cannot open the project at /path/to/my-app: `cuilo.json`: stack `import_style` value "js" must be `imports-extensionless`, `imports-js` or `imports-ts`
```

An empty list clears a fact. For example, `"formatter": []` stops cuilo from running any formatter on generated files, and `"import_style": []` keeps every relative import as the pack wrote it. Values must be plain names of up to 64 ASCII letters, digits and `-_.+#` characters. Anything else stops cuilo with an error:

```text
error: cannot open the project at /path/to/my-app: `cuilo.json`: stack value "my framework" must be a plain name such as `react`, `next.js` or `c++`
```

You can write the same settings in `cuilo.toml`, with a `[stack]` table, but a project cannot have both files. See the [`cuilo.json` reference](/reference/configuration/).
