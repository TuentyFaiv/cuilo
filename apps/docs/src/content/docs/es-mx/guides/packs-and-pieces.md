---

title: Packs y piezas
description: Los packs oficiales y sus piezas, y cómo nombrar una pieza, definir sus entradas, elegir una variante y decidir a dónde van sus archivos.
---

Un pack es un conjunto versionado de piezas. Una pieza es una cosa que puedes generar, como un componente o un recurso REST. Esta guía cubre los packs oficiales y las opciones que comparten todas las piezas.

## Los packs oficiales

`cuilo pack list` muestra los packs que puedes usar en el proyecto actual:

```sh
cuilo pack list
```

```text
backend 0.1.0  official:backend
  database, error, middleware, resource
ui 0.2.0  official:ui
  atom, molecule, organism
```

Los packs oficiales vienen dentro del binario de cuilo, así que un cuilo más nuevo trae su siguiente versión. Sus nombres, `ui` y `backend`, están reservados. Un pack de cualquier otro origen tiene que usar otro nombre.

| Pieza                | Qué genera                                                                   | Carpeta por defecto     |
| -------------------- | ---------------------------------------------------------------------------- | ----------------------- |
| `ui/atom`            | El bloque de UI más pequeño, como un badge, un ícono o una etiqueta          | `src/ui/atoms`          |
| `ui/molecule`        | Un grupo pequeño de átomos con una sola tarea, como un campo con su etiqueta | `src/ui/molecules`      |
| `ui/organism`        | Una sección propia de una interfaz, hecha de moléculas, como un encabezado   | `src/ui/organisms`      |
| `backend/resource`   | Un recurso REST por capas: modelo, repositorio, servicio, rutas y pruebas    | `src/modules/<plural>`  |
| `backend/error`      | Errores de la aplicación con formato problem details de RFC 9457             | `src/shared`            |
| `backend/middleware` | Middleware que propaga o crea un encabezado de la solicitud                  | `src/shared/middleware` |
| `backend/database`   | Un motor async de SQLAlchemy, sesiones y una base declarativa para FastAPI   | `app`                   |

Algunas variantes usan su propia carpeta. Por ejemplo, Flutter escribe en `lib/`, Axum en `src/` y las variantes de Go en `internal/`. Las páginas de [ui](/es-mx/packs/ui/) y [backend](/es-mx/packs/backend/) listan cada stack y cada carpeta.

## Nombra una pieza

Escribe una pieza como `pack/piece`. Si solo un pack cargado tiene una pieza con ese nombre, puedes omitir el pack:

```sh
cuilo add atom status-badge
```

Si varios packs la tienen, cuilo se detiene y te pide escribirla como `pack/piece`.

## Define las entradas

`cuilo pack show <pack>` lista cada pieza con sus entradas y variantes:

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

El argumento después de la pieza llena su entrada `name`. Puedes escribirlo con cualquier combinación de mayúsculas y minúsculas, así que `blog-post` y `BlogPost` dan los mismos archivos. Define las demás entradas con `-i key=value`, una vez por entrada. Las entradas de verdadero o falso aceptan `true` o `false`, `yes` o `no`, `y` o `n`, `1` o `0`, y `on` u `off`.

En un proyecto de Hono con TypeScript y Vitest:

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

La parte `(with error)` significa que `resource` se apoya en la pieza `error`. cuilo agrega `error` primero cuando el proyecto todavía no la tiene, y le pasa las entradas que comparten, como `tests`.

Si omites una entrada que no tiene valor por defecto, cuilo te la pide en una terminal:

```text
Plural name for routes, tables and folders, e.g. `blog-posts`:
```

Con `--no-input`, o sin una terminal, se detiene:

```text
error: input `plural` is required
```

cuilo nunca pregunta por las entradas de verdadero o falso. Toman su valor por defecto, y algunos valores por defecto dependen del stack. Por ejemplo, `tests` está activada cuando el proyecto puede correr las pruebas, como un proyecto de TypeScript con Vitest.

## Elige una variante

Cada pieza elige una variante según el [stack detectado](/es-mx/guides/stack-detection/). Para usar otra, pasa su nombre tal como aparece en `cuilo pack show`:

```sh
cuilo add ui/atom status-badge --variant svelte --dry-run
```

```text
add ui/atom
  create  src/ui/atoms/status-badge/status-badge.svelte
  create  src/ui/atoms/status-badge/index.ts
```

## Elige a dónde van los archivos

cuilo elige la carpeta en este orden:

1. `--target` en la línea de comandos.
2. `paths` en `cuilo.json`.
3. La carpeta propia de la variante, si tiene una.
4. La carpeta por defecto de la pieza.

```sh
cuilo add ui/atom status-badge --target src/ui/badges --dry-run
```

```text
add ui/atom
  create  src/ui/badges/status-badge/status-badge.module.css
  create  src/ui/badges/status-badge/status-badge.tsx
  create  src/ui/badges/status-badge/index.ts
```

Para fijar una carpeta en cada ejecución, agrega `paths` a `cuilo.json`. Usa `defaults` para definir valores de entradas. Las claves son nombres de piezas sin el pack:

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

Un valor que pasas con `-i` tiene prioridad sobre `defaults`. Consulta la [referencia de `cuilo.json`](/es-mx/reference/configuration/).

:::caution
Un pack solo puede escribir en las rutas que permite su `pack.toml`. El pack `ui` permite `src/ui/**` y las carpetas que usan sus variantes de Flutter, Compose y SwiftUI. Una carpeta fuera de esas rutas hace que se rechace el plan con el código de salida 3:

```text
The plan was rejected:
  src/components/status-badge/status-badge.module.css: the pack does not declare permission to write here
  src/components/status-badge/status-badge.tsx: the pack does not declare permission to write here
  src/components/status-badge/index.ts: the pack does not declare permission to write here
```
:::

## Usa otros packs

`--from <source>` carga un pack antes de generar a partir de él:

```sh
cuilo add mine/note release-plan --from ./packs/mine
```

Cuando aplicas ese plan, `cuilo.lock` fija el pack. A partir de ahí, `cuilo pack list` lo muestra, y los comandos siguientes lo encuentran por su nombre sin `--from`.

`cuilo pack add <source>` carga y verifica un pack sin generar nada:

```sh
cuilo pack add ./packs/mine
```

```text
Loaded mine 0.1.0 from path:./packs/mine (not signed by a trusted key)
```

Un origen puede ser una carpeta, un tag de GitHub (`gh:`), un repositorio de git por SSH (`git:`), un tarball por HTTPS, un elemento de registro (`registry:`) o un espacio de nombres de `cuilo.json`. Consulta [Orígenes de packs](/es-mx/packs/sources/), [Firma un pack](/es-mx/packs/signing/) y [Escribe un pack](/es-mx/packs/write-a-pack/).
