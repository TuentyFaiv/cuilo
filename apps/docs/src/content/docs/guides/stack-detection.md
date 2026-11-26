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

| Fact              | What it holds                                                          | Examples                                             |
| ----------------- | ---------------------------------------------------------------------- | ---------------------------------------------------- |
| `framework`       | Frameworks and UI libraries                                            | `react`, `next`, `svelte`, `hono`, `axum`, `flutter` |
| `language`        | Languages                                                              | `typescript`, `javascript`, `rust`, `python`, `go`   |
| `runtime`         | The JavaScript runtime, or Tokio for Rust                              | `node`, `bun`, `deno`, `tokio`                       |
| `database`        | The ORM, ODM or query builder that database code follows               | `drizzle`, `prisma`, `sqlx`, `sqlalchemy`, `gorm`    |
| `styling`         | CSS tools                                                              | `tailwind`, `sass`, `stylex`, `unocss`               |
| `package_manager` | Package managers                                                       | `npm`, `pnpm`, `bun`, `cargo`, `uv`, `gradle`        |
| `testing`         | Test libraries the project installed. Built-in runners are not listed. | `vitest`, `jest`, `playwright`, `pytest`             |
| `formatter`       | Formatters and fixing linters                                          | `biome`, `prettier`, `eslint`, `ruff`, `rustfmt`     |

A fact can hold several values, as `framework` does for a Next.js app. Database drivers, SQL files and schema files do not count as a `database`. `env` lists the variable names in `.env.example`. cuilo never reads their values.

## Where the facts come from

cuilo reads files at the project root: the current folder, or the one you pass with `--root`. Only packages the project declares count. Packages that arrive as dependencies of other packages do not.

| Ecosystem                 | Files                                                                                                |
| ------------------------- | ---------------------------------------------------------------------------------------------------- |
| JavaScript and TypeScript | `package.json`, `deno.json` or `deno.jsonc` imports, `tsconfig.json`, lockfiles                      |
| Rust                      | `Cargo.toml`, including dev, build and workspace dependencies                                        |
| Python                    | `pyproject.toml` (PEP 621, dependency groups and Poetry), `requirements.txt`, `Pipfile`, lockfiles   |
| Go                        | `go.mod`                                                                                             |
| Dart and Flutter          | `pubspec.yaml`                                                                                       |
| Ruby                      | `Gemfile`                                                                                            |
| PHP                       | `composer.json`                                                                                      |
| Kotlin and Java           | `build.gradle`, `build.gradle.kts`, `app/build.gradle(.kts)`, `gradle/libs.versions.toml`, `pom.xml` |
| Swift                     | `Package.swift`, `Podfile`, an `.xcodeproj` or `.xcworkspace`                                        |

A few rules decide the less obvious facts:

- `language` is `typescript` when `package.json` lists `typescript`, the project has a `tsconfig.json`, or it is a Deno project. Otherwise it is `javascript`.
- The JavaScript package manager comes from `packageManager` in `package.json`. Without it, cuilo looks for `bun.lock`, `pnpm-lock.yaml`, `yarn.lock` and `package-lock.json`, in that order. It falls back to `deno` in a Deno project and to `npm` otherwise.
- The Python package manager is `uv` with a `uv.lock`, `poetry` with Poetry settings or a `poetry.lock`, `pdm` with a `pdm.lock`, `pipenv` with a `Pipfile`, and `pip` otherwise.
- Frameworks with their own ORM add it: Rails adds `activerecord`, Laravel adds `eloquent` and Django adds `django-orm`.
- Formatters come from dependencies and from config files such as `biome.json`, `.prettierrc`, `eslint.config.js`, `dprint.json`, `ruff.toml` or `rustfmt.toml`, in a project of any language. Rust projects also get `rustfmt`, and Go projects get `gofmt`.
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

An empty list clears a fact. For example, `"formatter": []` stops cuilo from running any formatter on generated files. Values must be plain names of up to 64 ASCII letters, digits and `-_.+#` characters. Anything else stops cuilo with an error:

```text
error: cannot open the project at /path/to/my-app: `cuilo.json`: stack value "my framework" must be a plain name such as `react`, `next.js` or `c++`
```

You can write the same settings in `cuilo.toml`, with a `[stack]` table, but a project cannot have both files. See the [`cuilo.json` reference](/reference/configuration/).
