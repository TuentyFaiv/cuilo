---

title: Migrating from v1
description: What tfverse (v1) was, what changed in cuilo (v2), how to swap one for the other, and which old commands have a new equivalent.
---

cuilo 2 replaces tfverse, the v1 CLI. It is a new program with a new name, a new package and a new way of generating code. Most v1 options have no direct equivalent, and this page says so where that is the case.

:::caution[cuilo 2 is in alpha]
v2 is published as pre-releases until the beta. Your v1 install keeps working, and the two can be installed side by side because the commands have different names.
:::

## What v1 was

- **Name and package**: the command was `tfverse`, published on npm as `@tuentyfaiv/cli`. The last version was 0.0.11.
- **Install**: an npm `postinstall` script downloaded the binary for your platform from a storage bucket, with no checksum check.
- **What it generated**: base code for React, Svelte and vanilla JavaScript projects. It offered components (atoms, molecules, organisms or custom), pages, layouts, services and schemas, plus React HOCs, hooks and contexts, Svelte actions and stores, and vanilla classes. Not every kind was ready for every tool.
- **Projects**: with `project` or `library`, it cloned a boilerplate repository, reset its Git history and made a first commit.
- **Flow**: it asked questions or took flags, asked "Are you sure?", then wrote the files. It showed no preview of the contents, kept no record of what it generated, and had no undo or update.
- **Customizing**: a JSON config at `~/.tfverse/config_cli.json` (saved with `tfverse -g`), or a file passed with `-c`. It set paths, boilerplate repositories and folders of custom templates that used keywords such as `NAME_PASCAL`.

## What changed in v2

|                       | v1 (tfverse)                                                                 | v2 (cuilo)                                                                                                                                |
| --------------------- | ---------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| Command               | `tfverse`                                                                    | `cuilo`                                                                                                                                   |
| npm package           | `@tuentyfaiv/cli`                                                            | `@cuilo/cli`                                                                                                                              |
| npm install step      | `postinstall` downloads the binary                                           | No dependencies, nothing runs at install time                                                                                             |
| Other channels        | none                                                                         | Install scripts, Homebrew, cargo (see [Install](/install/))                                                                               |
| Where code comes from | Built-in templates and template folders                                      | Versioned [packs](/packs/write-a-pack/) from the official set, a folder, GitHub, an HTTPS tarball, `git:` over SSH or a registry item     |
| Stacks                | React, Svelte, vanilla JavaScript                                            | Detected from the project; the official packs cover web frameworks, Rust UI, Flutter, Jetpack Compose, SwiftUI and six backend frameworks |
| Before writing        | "Are you sure?"                                                              | A plan of every file, with `--dry-run`, `--diff` and `--review`                                                                           |
| After writing         | Nothing recorded                                                             | A transaction you can undo, and a lockfile that tracks generated files                                                                    |
| Pack updates          | none                                                                         | `cuilo update` merges the new version into files you edited                                                                               |
| Scripts and agents    | Flags for some answers; others, such as the component type, are always asked | `--json` and `--no-input` on every command, and `cuilo mcp` for AI agents                                                                 |
| Project creation      | Clones a boilerplate repository                                              | Not available: cuilo adds code to an existing project                                                                                     |

## Remove v1

Uninstall the package with the package manager you installed it with. Run the one line that matches:

```sh
npm uninstall --global @tuentyfaiv/cli   # npm
pnpm remove --global @tuentyfaiv/cli     # pnpm
yarn global remove @tuentyfaiv/cli       # Yarn 1
bun remove --global @tuentyfaiv/cli      # Bun
```

The binary lived inside the package folder, so this removes it too. Check that the command is gone:

```sh
command -v tfverse   # prints nothing once it is removed
```

If you ran `tfverse -g`, v1 saved its config in `~/.tfverse`. Delete that folder when you no longer need it:

```sh
rm -rf ~/.tfverse
```

## Install v2

Install the newest pre-release with npm, or with any other channel on [Install](/install/):

```sh
npm install --global @cuilo/cli
cuilo --version
```

## Old commands and new ones

v1 took one command with flags and asked for the rest. v2 has a command per task. The piece you generate is named `pack/piece`, and the stack comes from the project instead of flags.

### What to generate

| v1                                        | v2                                                                                                                                                           |
| ----------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `tfverse -a component`, type `atoms`      | `cuilo add ui/atom <name>`                                                                                                                                   |
| `tfverse -a component`, type `molecules`  | `cuilo add ui/molecule <name>`                                                                                                                               |
| `tfverse -a component`, type `organisms`  | `cuilo add ui/organism <name>`                                                                                                                               |
| `tfverse -a component`, type `custom`     | No official piece. [Write a pack](/packs/write-a-pack/), or turn a component you have into a piece with `cuilo pack capture`.                                |
| `tfverse -a project`, `-a library`        | No equivalent. cuilo does not create projects.                                                                                                               |
| `tfverse -a page`, `-a layout`            | No equivalent.                                                                                                                                               |
| `tfverse -a service`, `-a schema`         | No equivalent for front-end code. `backend/resource` generates a server-side REST resource (model, repository, service, routes), which is a different thing. |
| `tfverse -a hoc`, `-a hook`, `-a context` | No equivalent.                                                                                                                                               |
| `tfverse -a action`, `-a store`           | No equivalent.                                                                                                                                               |
| `tfverse -a class`                        | No equivalent.                                                                                                                                               |

Run `cuilo pack list` to see every piece, and `cuilo pack show ui` for a pack's pieces and their inputs.

### Flags

| v1                            | v2                                                                                                                                                                                       |
| ----------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `[NAME]`                      | `[name]`, the piece's `name` input, e.g. `cuilo add ui/atom status-badge`. Other inputs use `-i key=value`.                                                                              |
| `-t, --tool`                  | Detected from the project. `cuilo detect` shows what was found, `--variant` picks another variant, and `stack` in `cuilo.json` replaces detection.                                       |
| `-l, --language`              | Detected from the project, or set with `stack` in `cuilo.json`.                                                                                                                          |
| `-s, --styles`                | No flag. The `ui` pieces write CSS modules, or Tailwind classes when the project uses Tailwind.                                                                                          |
| `-p, --path`                  | `--target <folder>`. The folder must be one the pack may write; the `ui` pack writes under `src/ui/`, among others.                                                                      |
| `-y, --sure`                  | `-y, --yes` applies without asking.                                                                                                                                                      |
| `-g, --global`                | No equivalent questionnaire. `defaults` in `cuilo.json` sets input values per piece. Your [user config](/reference/user-config/) holds credentials, trusted signing keys and tool trust. |
| `-c, --config <FILE>`         | No flag. cuilo reads `cuilo.json` or `cuilo.toml` at the project root; `--root` picks the project.                                                                                       |
| `-h, --help`, `-V, --version` | The same.                                                                                                                                                                                |

### Config

| v1 config key              | v2                                                                                                                                            |
| -------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| `paths`                    | `paths` in `cuilo.json`, keyed by piece name, e.g. `{ "paths": { "atom": "src/ui/components" } }`. The folder must be one the pack may write. |
| `templates`                | A pack. Its templates are real code, and the example name is replaced in every case form instead of keywords such as `NAME_PASCAL`.           |
| `tools_type`, `repository` | No equivalent.                                                                                                                                |
| `lang`, `styles`, `i18n`   | No config key. The language comes from detection or `stack`, and styling follows the project, as the flags above describe.                    |

See [the project config reference](/reference/configuration/) for every `cuilo.json` field.

### New in v2

These have no v1 counterpart: `cuilo status`, `cuilo why`, `cuilo log`, `cuilo undo`, `cuilo update`, `cuilo adopt`, `cuilo pack`, `cuilo trust`, `cuilo mcp`, and the `--dry-run`, `--diff`, `--review`, `--verify`, `--json` and `--no-input` flags. The [CLI reference](/reference/cli/) lists them all.

## Code v1 generated

Files v1 wrote are plain code in your project. cuilo does not know about them, and you can keep them as they are.

To bring a file under cuilo, `cuilo adopt` takes over files at the paths a piece would write, so later updates merge into them. It never changes existing files and creates the ones that are missing. v1 used other file names and folders, so run it with `--dry-run` first to see which files line up:

```sh
cuilo adopt ui/atom status-badge --dry-run
```

To reuse a v1 template, generate a component with it, then capture that component as a piece of a local pack. The name you pass with `--name` must have two or more words:

```sh
cuilo pack capture src/ui/atoms/user-card/user-card.tsx --piece card --name UserCard
```

[Adopt and capture](/guides/adopt-and-capture/) covers both commands.
