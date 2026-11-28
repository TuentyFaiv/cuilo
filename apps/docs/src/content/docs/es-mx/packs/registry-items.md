---

title: Elementos de registro
description: Carga elementos en el formato registry-item.json como packs y define dónde van sus archivos con los ajustes de registry en cuilo.json.
---

Un origen `registry:` carga un elemento de registro (registry item) en el formato `registry-item.json` y lo convierte en un pack. El elemento y cada elemento que requiere se vuelven piezas, así que los revisas, aplicas, deshaces y actualizas como cualquier otra pieza.

## Agrega un elemento

Pasa la URL HTTPS del elemento; su ruta tiene que terminar en `.json`:

```sh
cuilo add card --from registry:https://acme.dev/r/card.json
```

El pack se llama `registry-<item>`, aquí `registry-card`, y cada pieza lleva el nombre de su elemento. Los nombres de los elementos usan letras minúsculas, dígitos y `-`, con un máximo de 56 caracteres.

Para usar nombres cortos, asocia un espacio de nombres a una plantilla de URL en `cuilo.json`, donde `{name}` representa el elemento:

```json
{
	"registry": {
		"namespaces": { "@acme": "https://acme.dev/r/{name}.json" }
	}
}
```

Después, `registry:@acme/card` carga `https://acme.dev/r/card.json`. Un espacio de nombres tiene que dar una URL HTTPS. Los espacios de nombres del registro no son los mismos que los espacios de nombres de [`sources`](/es-mx/packs/sources/#espacio-de-nombres) para packs.

Los registros privados usan la tabla `auth` de tu configuración de usuario, con el host de cada URL como clave; consulta [orígenes privados](/es-mx/packs/sources/#orígenes-privados).

## Qué arma cuilo

Toma este elemento en `https://acme.dev/r/card.json`:

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

- Los `registryDependencies` también se vuelven piezas, y `card` las requiere. Un nombre sin URL, como `utils`, se descarga de junto al elemento que lo requiere: `https://acme.dev/r/utils.json`. Una dependencia también puede ser una URL HTTPS o `@namespace/name`. Un elemento puede traer como máximo 48 elementos.
- `dependencies` y `devDependencies` son paquetes de npm que lista el plan. `clsx@^2.1` pide `^2.1`; un paquete sin versión pide `latest`.
- Los archivos sin `content` se omiten.
- La versión del pack es `0.0.0-h` seguida de 12 dígitos hexadecimales de un hash de todo lo descargado, así que cambia justo cuando cambia el contenido.
- El pack solo puede escribir los archivos que coloca, más la hoja de estilos de las variables CSS.

En un proyecto con carpeta `src` y sin ajustes de `registry`, `card.tsx` queda en `src/components/ui/card.tsx`, su import dice `import { cn } from "@/lib/utils"` y se quita el `"use client"` del principio.

## Dónde van los archivos

El `type` de cada archivo elige un alias:

| `type`                                 | Alias        | Por defecto       |
| -------------------------------------- | ------------ | ----------------- |
| `registry:ui`                          | `ui`         | `@/components/ui` |
| `registry:component`, `registry:block` | `components` | `@/components`    |
| `registry:lib`                         | `lib`        | `@/lib`           |
| `registry:hook`                        | `hooks`      | `@/hooks`         |

Los demás tipos necesitan un `target` en el elemento. Un `target` que empieza con `@components/`, `@ui/`, `@lib/` o `@hooks/` va bajo ese alias; si no, es una ruta desde la raíz del proyecto, sin el `~/` inicial si lo tiene. Una ruta que se sale del proyecto se rechaza.

cuilo convierte un alias en una carpeta con `compilerOptions.paths` de `tsconfig.json`, o de `jsconfig.json`, cuando alguno coincide. Así, con `"~molecules/*": ["./src/ui/molecules/*"]` y el alias `~molecules`, los archivos quedan en `src/ui/molecules/`. Si no, `@/` y `~/` significan `src/` cuando el proyecto tiene una carpeta `src`, y la raíz del proyecto cuando no la tiene.

Los imports escritos como especificadores `@/…` entre comillas, incluidos los `@/registry/<style>/…`, se reescriben con los alias del proyecto: `@/lib/utils` a `utils`; `@/lib/…` a `lib`; `@/hooks/…` a `hooks`; `@/ui/…` y `@/components/ui/…` a `ui`; y los demás `@/components/…` a `components`. Los demás imports `@/` se quedan como están.

## Ajustes en cuilo.json

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

- `aliases` define `components`, `ui`, `lib`, `utils` y `hooks`. Los que no definas toman por defecto `@/components`, `<components>/ui`, `@/lib`, `<lib>/utils` y `@/hooks`.
- `css` es la hoja de estilos a la que van las variables CSS de los elementos. Solo se usa cuando el archivo existe. Las variables `light` van a `:root`, las `dark` a `.dark` y las `theme` a `@theme inline`, como [parches `css_vars`](/es-mx/packs/write-a-pack/#parches) que conservan los valores que el proyecto ya definió.
- `server_components: true` conserva las directivas `"use client"`, para proyectos con React Server Components.
- `namespaces` asocia `@namespace` a una plantilla de URL con `{name}`.

Todas las claves están en la [referencia de configuración](/es-mx/reference/configuration/#registry).

## Qué no se aplica todavía

cuilo todavía no aplica `css`, `tailwind` ni `envVars` de un elemento, ni sus `cssVars` cuando `registry.css` no está definido. La descripción del pack indica lo que omitió, por ejemplo ``Not applied by cuilo yet; add them to your CSS, Tailwind config or .env yourself: `cssVars` of `utils`.`` Puedes verla con `cuilo pack add registry:https://acme.dev/r/card.json --json`.

## Actualiza un elemento

La versión sale del contenido descargado, así que corre `cuilo update` con el mismo origen para volver a descargar el elemento y fusionarlo como cualquier otro pack:

```sh
cuilo update registry-card --from registry:https://acme.dev/r/card.json
```

:::note
cuilo arma los packs de registro y no llevan firma, así que se rechazan cuando tu configuración de usuario define `signing.require = true`. Consulta [firma un pack](/es-mx/packs/signing/).
:::
