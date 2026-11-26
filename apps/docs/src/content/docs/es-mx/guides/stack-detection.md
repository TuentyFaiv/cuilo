---

title: Detección del stack
description: Cómo lee cuilo los archivos de tu proyecto para conocer su stack, cómo el stack decide el código que genera cada pieza y cómo sobrescribirlo.
---

Antes de planear una pieza, cuilo lee los archivos de manifiesto de tu proyecto y arma una lista de datos del stack. Las piezas usan esos datos para elegir una variante y para conservar u omitir archivos y líneas.

## Mira lo que detecta cuilo

Toma un proyecto de Next.js con este `package.json` y un `.env.example` que define `DATABASE_URL` y `API_TOKEN`:

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

`cuilo detect` siempre imprime JSON. Solo lee archivos y no corre nada.

## Los datos

| Dato              | Qué contiene                                                                           | Ejemplos                                             |
| ----------------- | -------------------------------------------------------------------------------------- | ---------------------------------------------------- |
| `framework`       | Frameworks y bibliotecas de UI                                                         | `react`, `next`, `svelte`, `hono`, `axum`, `flutter` |
| `language`        | Lenguajes                                                                              | `typescript`, `javascript`, `rust`, `python`, `go`   |
| `runtime`         | El runtime de JavaScript, o Tokio para Rust                                            | `node`, `bun`, `deno`, `tokio`                       |
| `database`        | El ORM, ODM o query builder que sigue el código de base de datos                       | `drizzle`, `prisma`, `sqlx`, `sqlalchemy`, `gorm`    |
| `styling`         | Herramientas de CSS                                                                    | `tailwind`, `sass`, `stylex`, `unocss`               |
| `package_manager` | Gestores de paquetes                                                                   | `npm`, `pnpm`, `bun`, `cargo`, `uv`, `gradle`        |
| `testing`         | Bibliotecas de pruebas que instaló el proyecto. Los runners integrados no aparecen.    | `vitest`, `jest`, `playwright`, `pytest`             |
| `formatter`       | Formateadores y linters que corrigen el código                                         | `biome`, `prettier`, `eslint`, `ruff`, `rustfmt`     |
| `import_style`    | Cómo escribe los imports relativos el JavaScript y TypeScript generado. Un solo valor. | `imports-extensionless`, `imports-js`, `imports-ts`  |

Un dato puede tener varios valores, como `framework` en una app de Next.js. Los drivers de bases de datos, los archivos SQL y los archivos de esquema no cuentan como `database`. `env` lista los nombres de las variables de `.env.example`. cuilo nunca lee sus valores.

## De dónde salen los datos

cuilo lee archivos en la raíz del proyecto: la carpeta actual, o la que pasas con `--root`. Solo cuentan los paquetes que declara el proyecto. Los que llegan como dependencias de otros paquetes no cuentan.

| Ecosistema              | Archivos                                                                                                             |
| ----------------------- | -------------------------------------------------------------------------------------------------------------------- |
| JavaScript y TypeScript | `package.json`, imports de `deno.json` o `deno.jsonc`, `tsconfig.json` y las configuraciones que extiende, lockfiles |
| Rust                    | `Cargo.toml`, incluidas las dependencias de dev, build y workspace                                                   |
| Python                  | `pyproject.toml` (PEP 621, grupos de dependencias y Poetry), `requirements.txt`, `Pipfile`, lockfiles                |
| Go                      | `go.mod`                                                                                                             |
| Dart y Flutter          | `pubspec.yaml`                                                                                                       |
| Ruby                    | `Gemfile`                                                                                                            |
| PHP                     | `composer.json`                                                                                                      |
| Kotlin y Java           | `build.gradle`, `build.gradle.kts`, `app/build.gradle(.kts)`, `gradle/libs.versions.toml`, `pom.xml`                 |
| Swift                   | `Package.swift`, `Podfile`, un `.xcodeproj` o `.xcworkspace`                                                         |

Algunas reglas deciden los datos menos obvios:

- `language` es `typescript` cuando `package.json` lista `typescript`, el proyecto tiene un `tsconfig.json` o es un proyecto de Deno. Si no, es `javascript`.
- El gestor de paquetes de JavaScript sale de `packageManager` en `package.json`. Si no está, cuilo busca `bun.lock`, `pnpm-lock.yaml`, `yarn.lock` y `package-lock.json`, en ese orden. Si no encuentra ninguno, usa `deno` en un proyecto de Deno y `npm` en cualquier otro.
- El gestor de paquetes de Python es `uv` si hay un `uv.lock`, `poetry` si hay configuración de Poetry o un `poetry.lock`, `pdm` si hay un `pdm.lock`, `pipenv` si hay un `Pipfile`, y `pip` en cualquier otro caso.
- Los frameworks con ORM propio lo agregan: Rails agrega `activerecord`, Laravel agrega `eloquent` y Django agrega `django-orm`.
- Los formateadores salen de las dependencias y de archivos de configuración como `biome.json`, `.prettierrc`, `eslint.config.js`, `dprint.json`, `ruff.toml` o `rustfmt.toml`, en un proyecto de cualquier lenguaje. Los proyectos de Rust también reciben `rustfmt`, y los de Go reciben `gofmt`.
- `import_style` sale del `tsconfig.json` de la raíz, o de Deno. Luego cada pieza usa el de su propia carpeta. Consulta [Imports relativos en el código generado](#imports-relativos-en-el-código-generado).
- Otros ecosistemas, como Elixir o .NET, no reciben ningún dato.

## Cómo eligen el código las variantes

Una pieza lista sus variantes en orden, cada una con los datos que necesita. Esta parte del manifiesto de `ui/atom` muestra tres:

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

cuilo usa la primera variante cuyos datos se cumplen todos. El proyecto de Next.js de arriba tiene `react` y `typescript`, así que recibe la variante `react`.

Dentro de una variante, las condiciones sobre `stack.<value>` conservan o quitan archivos y líneas. El átomo omite su módulo CSS cuando se cumple `stack.tailwind`, y su entrada `tests` está activada cuando se cumple `stack.vitest`. Así que en el proyecto de Next.js el plan trae un archivo de pruebas y ningún módulo CSS:

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

La nota se refiere a correr Biome sobre los archivos generados. Consulta [Formateadores y confianza](/es-mx/guides/formatters-and-trust/).

Cuando ninguna variante coincide, cuilo lista las que tiene la pieza. Un proyecto de React sin TypeScript recibe esto, porque la variante `react` necesita `typescript`:

```text
error: no variant matches this project; available: angular, qwik, solid, astro, svelte, vue, react, lit, leptos, dioxus, yew, flutter, compose-multiplatform, compose, swiftui-package, swiftui
```

Para elegir una variante en un solo comando, pasa `--variant`. Consulta [Packs y piezas](/es-mx/guides/packs-and-pieces/#elige-una-variante).

`cuilo.lock` registra el stack, la variante y la carpeta con los que se generó cada pieza. [`cuilo update`](/es-mx/guides/updates/) genera la versión nueva del pack con esos mismos valores, así que si después cambias tu stack, una pieza que ya tienes no se pasa a otra variante.

## Imports relativos en el código generado

Los packs escriben los imports relativos sin extensión, como `./blog-post.service`. Eso funciona donde un bundler resuelve los archivos, como en Vite, Astro, SvelteKit, Next.js, Bun y Wrangler. La resolución `NodeNext` de TypeScript necesita la extensión que tiene el archivo ya compilado, y Deno necesita la extensión del propio archivo. Por eso cuilo escribe cada import relativo como lo exige el código del proyecto, y lo registra en el dato `import_style`:

| `import_style`          | Un import de `blog-post.service.ts` | Cuándo                                                                                                                  |
| ----------------------- | ----------------------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| `imports-extensionless` | `./blog-post.service`               | `moduleResolution` es `bundler`, `node10`, `node` o `classic`, o no hay `tsconfig.json`                                 |
| `imports-js`            | `./blog-post.service.js`            | `moduleResolution` es `nodenext` o `node16`, o no está definido y `module` es `node16`, `node18`, `node20` o `nodenext` |
| `imports-ts`            | `./blog-post.service.ts`            | `allowImportingTsExtensions` o `rewriteRelativeImportExtensions` es `true`, o el proyecto usa Deno                      |

El archivo sigue llamándose `blog-post.service.ts`. Con `imports-js`, un import de un archivo `.mts` termina en `.mjs`, y uno de un archivo `.cts`, en `.cjs`. Con `imports-ts`, cada import termina en la extensión del archivo que nombra, como `.tsx`. Las plantillas de Node.js y Vercel de create-hono usan `NodeNext`, así que ahí el pack `backend` escribe `./blog-post.service.js`, mientras que sus plantillas de Bun y Cloudflare Workers conservan `./blog-post.service`.

cuilo lee el `tsconfig.json` más cercano a la carpeta a la que va una pieza, subiendo hasta la raíz del proyecto, así que un paquete de un monorepo puede tener su propio estilo. Sigue `extends`: rutas relativas, paquetes en `node_modules` (por su `exports`, su campo `tsconfig` o su `tsconfig.json`) y listas de ambos, donde ganan las entradas posteriores. Para leer estas configuraciones y el `package.json` de cada paquete, cuilo sigue enlaces simbólicos y uniones de directorio (junctions) de Windows, como los que pnpm crea en `node_modules`, pero solo cuando el archivo real está dentro del proyecto. Todos los demás archivos del proyecto los lee y escribe sin seguir enlaces. Un `deno.json` o `deno.jsonc` en el camino, o un runtime `deno`, indica Deno. cuilo nunca lee una configuración fuera del proyecto, se detiene después de 16 niveles de `extends` y omite las configuraciones de más de 1 MiB. Las que no puede leer, y los enlaces rotos, circulares o que llevan fuera del proyecto, cuentan como ausentes. `cuilo detect` muestra el estilo de la raíz del proyecto.

Cómo reescribe cuilo los imports:

- Busca el archivo que nombra cada import relativo entre los archivos que escribe el plan y los que ya tiene el proyecto. `./blog-post.service` nombra `blog-post.service.ts`, así que un punto en el nombre de un archivo no es una extensión. `./shared` nombra `shared/index.ts` cuando no hay `shared.ts`, y queda como `./shared/index.js`.
- Cubre las sentencias `import` y `export … from`, `import type`, `import "…"`, `export * from`, el `import("…")` dinámico y, en archivos CommonJS, `require("…")`. Reescribe los archivos `.ts`, `.tsx`, `.mts`, `.cts`, `.js`, `.jsx`, `.mjs` y `.cjs` generados, y el texto que los parches `import` y `anchor` agregan a esos archivos, incluido el cableado.
- Los imports que ya terminan en `.js`, `.ts` u otra extensión de script quedan igual. También los imports de otros archivos, como `./styles.css` o `./card.svelte`, de paquetes y de alias como `@/lib/utils`, `~/lib` o `#lib`. Los comentarios, las cadenas y las líneas con directivas `@cuilo` nunca cambian.
- Un import relativo que no nombra ningún archivo queda como está escrito, y el plan lo anota. Por ejemplo, con la pieza `error` movida a `src/errors` mediante `paths`:

  ```text
  Notes:
    src/modules/blog-posts/blog-post.routes.ts: no file matches the import `../../shared/errors`, so cuilo left it as the pack wrote it
  ```

- La reescritura corre antes que el formateador del proyecto, así que Biome o Prettier ven el código final.

`cuilo.lock` registra el estilo con el stack de cada pieza, y `cuilo update` sigue escribiendo sus imports de esa forma aunque cambie `tsconfig.json`, para que el original que reconstruye para la fusión coincida con lo que escribió. Las piezas registradas sin estilo conservan los imports sin extensión. Para elegir el estilo tú, define `import_style` en `cuilo.json`, como muestra la siguiente sección.

## Sobrescribe la detección

Agrega `stack` a `cuilo.json` cuando la detección se equivoca con tu proyecto. Cada dato que listas reemplaza los valores detectados para ese dato. Los demás datos se siguen detectando:

```json title="cuilo.json"
{
	"stack": { "framework": ["svelte"] }
}
```

En el proyecto de React de [Tu primera pieza](/es-mx/first-piece/), `cuilo detect` ahora muestra:

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

Y el átomo sale como un componente de Svelte:

```text
add ui/atom
  create  src/ui/atoms/status-badge/status-badge.svelte
  create  src/ui/atoms/status-badge/index.ts
```

`import_style` funciona igual. Si lo defines, cuilo no lee ningún `tsconfig.json` para elegir el estilo, y escribe los imports relativos de todas las piezas en el que elegiste:

```json title="cuilo.json"
{
	"stack": { "import_style": ["imports-js"] }
}
```

Acepta un solo valor: `imports-extensionless`, `imports-js` o `imports-ts`. Cualquier otra cosa detiene a cuilo con un error:

```text
error: cannot open the project at /path/to/my-app: `cuilo.json`: stack `import_style` value "js" must be `imports-extensionless`, `imports-js` or `imports-ts`
```

Una lista vacía borra un dato. Por ejemplo, `"formatter": []` hace que cuilo no corra ningún formateador sobre los archivos generados, y `"import_style": []` deja cada import relativo como lo escribió el pack. Los valores tienen que ser nombres simples de hasta 64 caracteres: letras ASCII, dígitos y `-_.+#`. Cualquier otra cosa detiene a cuilo con un error:

```text
error: cannot open the project at /path/to/my-app: `cuilo.json`: stack value "my framework" must be a plain name such as `react`, `next.js` or `c++`
```

Puedes escribir la misma configuración en `cuilo.toml`, con una tabla `[stack]`, pero un proyecto no puede tener los dos archivos. Consulta la [referencia de `cuilo.json`](/es-mx/reference/configuration/).
