---

title: Packages a piece needs
description: How plans list the packages a piece imports, and how cuilo offers to add them with your project's own package manager.
---

A piece can name the packages its code imports, such as `clsx` for a component or `zod` for a resource. cuilo lists them in the plan and, after applying, offers the command that adds the missing ones with your project's package manager.

## In the plan

Packages appear under their own heading, with their ecosystem and version range. Development packages are marked `(dev)`:

```text
add pyx/client
  create  app/client.py
Packages to add (offered after applying; --install adds them without asking):
  pypi     httpx >=0.28
  pypi     pytest >=8 (dev)
```

An update lists the packages the new version adds. See [Update generated code](/guides/updates/).

A pack must set `permissions.dependencies = true` in `pack.toml` to declare packages. Otherwise cuilo refuses to load it:

```text
error: `pieces/card/piece.toml`: declares dependencies, but `pack.toml` does not set `permissions.dependencies`
```

To declare packages in your own pack, see [Write a pack](/packs/write-a-pack/) and the [piece manifest](/reference/piece-manifest/).

## After applying

cuilo applies the plan first. Then it builds the command for the packages your project does not declare yet, and asks:

```text
Apply these changes? [y/N] y
Applied transaction 1 (1 file(s)). Undo it with `cuilo undo`.
The plan lists packages to add:
  bun add clsx@^2.1
Run this? [y/N]
```

If you answer no, cuilo prints the command to run yourself:

```text
Add the packages with:
  bun add clsx@^2.1
```

Packages the project already declares are skipped, so their versions never change. Here `httpx` is already in `pyproject.toml`, so only `pytest` is left:

```text
Add the packages with:
  uv add --dev 'pytest>=8'
```

When every package is declared, cuilo asks nothing.

## Choose without a prompt

| Flag           | What happens                                     |
| -------------- | ------------------------------------------------ |
| `--install`    | Runs the command after applying, without asking  |
| `--no-install` | Never offers it                                  |
| neither        | Asks in a terminal; otherwise prints the command |

With `--install`, cuilo prints each command before running it from the project root:

```text
Applied transaction 2 (1 file(s)). Undo it with `cuilo undo`.
$ bun add clsx@^2.1
```

If the command fails, the generated files stay applied, and cuilo exits with code `1`:

```text
error: `bun add clsx@^2.1` failed (exit status: 1); the generated files are applied, so add the packages yourself
```

With `--json`, the result has an `install` object with the `commands`, the packages already `present`, the `manual` ones, and whether the commands `ran`.

## Package managers

cuilo uses the package manager your project already uses:

| Ecosystem   | Package manager              | Command                                                            |
| ----------- | ---------------------------- | ------------------------------------------------------------------ |
| `npm`       | bun, pnpm, yarn, deno or npm | `bun add`, `pnpm add`, `yarn add`, `deno add npm:…`, `npm install` |
| `cargo`     | cargo                        | `cargo add`, one package at a time                                 |
| `pypi`      | uv, Poetry, PDM or Pipenv    | `uv add`, `poetry add`, `pdm add`, `pipenv install`                |
| `go`        | go                           | `go get`                                                           |
| `pub`       | Flutter or Dart              | `flutter pub add`, else `dart pub add`                             |
| `rubygems`  | Bundler                      | `bundle add`                                                       |
| `packagist` | Composer                     | `composer require`                                                 |

Development packages get each tool's own flag, such as `-d` for bun, `--dev` for cargo and uv, or `--group dev` for Poetry.

For JavaScript, the `packageManager` field in `package.json` decides. Without it, the lockfile does: `bun.lock`, `pnpm-lock.yaml`, `yarn.lock` or `package-lock.json`. With none of them, cuilo uses deno in a Deno project and npm otherwise. For Python, it is `uv.lock`, Poetry's config or `poetry.lock`, `pdm.lock` or `Pipfile`. [`cuilo detect`](/reference/cli/detect/) shows what cuilo found.

When there is no package manager cuilo can run, such as a Python project with only `requirements.txt`, it tells you what to add by hand:

```text
Add by hand: add `httpx>=0.28` to requirements.txt or pyproject.toml
Add by hand: add `pytest>=8` to requirements.txt or pyproject.toml
```

## Undo and installs

The package manager's changes are outside cuilo's history. `cuilo undo` reverts the generated files and the lockfile, but not the packages that were added. Remove those with your package manager. See [History and undo](/guides/history/).
