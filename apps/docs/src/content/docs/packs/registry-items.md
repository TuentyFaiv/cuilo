---

title: Registry items
description: Load items in the registry-item.json format as packs, and set where their files go with the registry settings in cuilo.json.
---

A `registry:` source loads an item in the `registry-item.json` format and turns it into a pack. The item and every item it requires become pieces, so you review, apply, undo and update them like any other piece.

## Add an item

Give the item's HTTPS URL; its path must end in `.json`:

```sh
cuilo add card --from registry:https://acme.dev/r/card.json
```

The pack is named `registry-<item>`, here `registry-card`, and each piece is named after its item. Item names are lowercase letters, digits and `-`, up to 56 characters.

To use short names, map a namespace to a URL template in `cuilo.json`, where `{name}` stands for the item:

```json
{
	"registry": {
		"namespaces": { "@acme": "https://acme.dev/r/{name}.json" }
	}
}
```

Then `registry:@acme/card` loads `https://acme.dev/r/card.json`. A namespace must give an HTTPS URL. Registry namespaces are separate from the [`sources`](/packs/sources/#namespace) namespaces for packs.

Private registries use the `auth` table of your user config, keyed by the host of each URL; see [private sources](/packs/sources/#private-sources).

## What cuilo builds

Take this item at `https://acme.dev/r/card.json`:

```json
{
	"name": "card",
	"type": "registry:ui",
	"dependencies": ["clsx@^2.1"],
	"registryDependencies": ["utils"],
	"files": [
		{
			"path": "registry/default/ui/card.tsx",
			"type": "registry:ui",
			"content": "\"use client\"\n\nimport { cn } from \"@/registry/default/lib/utils\"\n…"
		}
	]
}
```

- `registryDependencies` become pieces too, and `card` requires them. A name without a URL, like `utils`, is fetched from beside the item that requires it: `https://acme.dev/r/utils.json`. A dependency can also be an HTTPS URL or `@namespace/name`. One item can pull in at most 48 items.
- `dependencies` and `devDependencies` are npm packages the plan lists. `clsx@^2.1` asks for `^2.1`; a package without a version asks for `latest`.
- Files without `content` are skipped.
- The pack's version is `0.0.0-h` followed by 12 hex digits of a hash of everything fetched, so it changes exactly when the content does.
- The pack may write only the files it places, plus the stylesheet for CSS variables.

In a project with a `src` folder and no `registry` settings, `card.tsx` lands at `src/components/ui/card.tsx`, its import reads `import { cn } from "@/lib/utils"`, and the leading `"use client"` is removed.

## Where files go

Each file's `type` picks an alias:

| `type`                                 | Alias        | Default           |
| -------------------------------------- | ------------ | ----------------- |
| `registry:ui`                          | `ui`         | `@/components/ui` |
| `registry:component`, `registry:block` | `components` | `@/components`    |
| `registry:lib`                         | `lib`        | `@/lib`           |
| `registry:hook`                        | `hooks`      | `@/hooks`         |

Other types need a `target` in the item. A `target` that starts with `@components/`, `@ui/`, `@lib/` or `@hooks/` goes under that alias; otherwise it is a path from the project root, with any leading `~/` removed. A path that leaves the project is refused.

cuilo turns an alias into a folder through `compilerOptions.paths` in `tsconfig.json`, or `jsconfig.json`, when one matches. So with `"~molecules/*": ["./src/ui/molecules/*"]` and the alias `~molecules`, files land in `src/ui/molecules/`. Otherwise `@/` and `~/` mean `src/` when the project has a `src` folder, and the project root when it does not.

Imports written as quoted `@/…` specifiers, including `@/registry/<style>/…`, are rewritten to the project's aliases: `@/lib/utils` to `utils`; `@/lib/…` to `lib`; `@/hooks/…` to `hooks`; `@/ui/…` and `@/components/ui/…` to `ui`; and other `@/components/…` to `components`. Other `@/` imports stay as written.

## Settings in cuilo.json

```json
{
	"registry": {
		"aliases": { "components": "@/components", "utils": "@/lib/utils" },
		"css": "src/app/globals.css",
		"server_components": true,
		"namespaces": { "@acme": "https://acme.dev/r/{name}.json" }
	}
}
```

- `aliases` sets `components`, `ui`, `lib`, `utils` and `hooks`. Unset ones default to `@/components`, `<components>/ui`, `@/lib`, `<lib>/utils` and `@/hooks`.
- `css` is the stylesheet that items' CSS variables go into. It is used only when the file exists. `light` variables go into `:root`, `dark` into `.dark` and `theme` into `@theme inline`, as [`css_vars` patches](/packs/write-a-pack/#patches) that keep values the project already set.
- `server_components: true` keeps `"use client"` directives, for projects with React Server Components.
- `namespaces` maps `@namespace` to a URL template with `{name}`.

Every key is in the [configuration reference](/reference/configuration/#registry).

## What is not applied yet

cuilo does not apply an item's `css`, `tailwind` or `envVars`, nor its `cssVars` when `registry.css` is not set. The pack's description names what it skipped, e.g. ``Not applied by cuilo yet; add them to your CSS, Tailwind config or .env yourself: `cssVars` of `utils`.`` See it with `cuilo pack add registry:https://acme.dev/r/card.json --json`.

## Update an item

The version comes from the fetched content, so run `cuilo update` with the same source to fetch the item again and merge it like any other pack:

```sh
cuilo update registry-card --from registry:https://acme.dev/r/card.json
```

:::note
Registry packs are built by cuilo and carry no signature, so they are refused when your user config sets `signing.require = true`. See [signing](/packs/signing/).
:::
