---

title: Introducción
description: Qué hace cuilo, las ideas detrás (packs, piezas, variantes, planes, el lockfile y el historial) y por dónde empezar.
---

cuilo genera código en tu proyecto a partir de **packs** versionados: componentes, recursos REST, middleware y otras piezas de una app. Ves cada cambio como un plan antes de que se escriba nada. El código que agrega es tuyo y puedes editarlo. Cuando el pack publica una versión nueva, `cuilo update` fusiona los cambios con los archivos que editaste.

:::note
cuilo 2 está en alfa: [instálalo](/es-mx/install/) para probarlo, y espera cambios incompatibles hasta la beta.
:::

## Packs y piezas

Un **pack** es una carpeta versionada de piezas con un `pack.toml` que le da nombre, indica su versión y lista las rutas en las que puede escribir. Dos packs oficiales vienen dentro del binario de cuilo:

- `ui`: componentes de diseño atómico (`atom`, `molecule`, `organism`) para Svelte, Astro, Leptos, Dioxus, Yew, SwiftUI, Angular, Qwik, Solid, Vue, React, Lit, Flutter y Jetpack Compose (Android y Multiplatform).
- `backend`: un `resource` REST por capas, respuestas de `error` según RFC 9457, `middleware` para encabezados de la solicitud y una configuración de `database`, para Axum, Hono, Express, FastAPI, chi y Echo.

Una **pieza** es una cosa que un pack puede generar. La nombras como `pack/piece`:

```sh
cuilo add ui/atom status-badge
```

Los packs también pueden venir de una carpeta, un tag de git, un tarball por HTTPS o un elemento de registro. Consulta [Orígenes de packs](/es-mx/packs/sources/).

## Entradas

Cada pieza declara **entradas** tipadas: texto, verdadero o falso, una opción de una lista o un número. El argumento `name` llena la entrada `name`, y `-i key=value` define las demás. En una terminal, cuilo te pide las entradas que no pasaste y que no tienen valor por defecto. Con `--no-input`, en vez de preguntar, falla. La misma definición también les da a los agentes de IA un esquema por MCP.

## Variantes

Las piezas se adaptan a tu stack con **variantes**. cuilo lee los archivos de tu proyecto para saber su framework, lenguaje, estilos, biblioteca de pruebas y más ([`cuilo detect`](/es-mx/reference/cli/detect/) muestra el resultado). Cada variante nombra los datos que necesita, y cuilo usa la primera que coincide. Un proyecto con Svelte y TypeScript recibe un componente `.svelte`. Un proyecto de Leptos recibe un componente de Rust en `src/ui/atoms/`. Si usas Tailwind, el componente recibe clases de utilidad en lugar de sus propios estilos. Consulta [Detección del stack](/es-mx/guides/stack-detection/).

## El plan

Los comandos que cambian archivos primero arman un **plan**. El plan lista cada archivo que se va a crear, modificar, reemplazar o borrar, los paquetes que importa el código y algunas notas. Las rutas riesgosas, como `package.json` o los workflows de CI, quedan marcadas. Un pack solo puede escribir en las rutas que permite su `pack.toml`, y cualquier otra ruta hace que se rechace el plan.

Tú decides qué pasa después:

- `--dry-run` muestra el plan y se detiene.
- `--diff` agrega el diff de cada archivo que cambia.
- `--review` te deja conservar u omitir cada fragmento, como `git add -p`.
- `--verify` primero corre las revisiones del propio proyecto en una copia desechable. Consulta [Verifica un plan](/es-mx/guides/verify/).
- `--yes` aplica sin preguntar.

## Historial y deshacer

Aplicar un plan escribe una **transacción**. cuilo guarda su historial en `.cuilo/`, en la raíz del proyecto, una carpeta que se ignora a sí misma en Git. [`cuilo log`](/es-mx/reference/cli/log/) lista las transacciones y [`cuilo undo`](/es-mx/reference/cli/undo/) revierte la última. `cuilo undo` se niega cuando un archivo cambió después de la transacción, para que no pierdas por accidente los cambios que hiciste después. Consulta [Historial y deshacer](/es-mx/guides/history/).

## El lockfile

`cuilo.lock` es el archivo que va en tus commits. Fija cada pack por su origen y el SHA-256 de sus archivos, así que cuilo rechaza un origen que después entregue archivos distintos para lo que ya quedó fijado. Por cada pieza generada, registra la variante, la carpeta de destino, el stack detectado, las entradas y el hash de cada archivo tal como cuilo lo escribió.

Ese registro le permite a cuilo distinguir tu código del código del pack:

- [`cuilo status`](/es-mx/reference/cli/status/) muestra cada archivo generado como `clean`, `modified` o `missing`.
- [`cuilo why path:line`](/es-mx/reference/cli/why/) te dice si una línea vino del pack o de ti.

Consulta la [referencia del lockfile](/es-mx/reference/lockfile/).

## Actualizaciones que se fusionan

[`cuilo update <pack>`](/es-mx/reference/cli/update/) pasa todas las piezas de un pack a una versión nueva en un solo plan. Los archivos que nunca tocaste toman la versión nueva. Los archivos que editaste se fusionan en tres vías, línea por línea, con el original que escribió cuilo como base. Cuando tú y el pack cambian las mismas líneas, por defecto se rechaza el plan. También puedes escribir marcadores al estilo de Git, conservar tus líneas o tomar las del pack. Consulta [Actualiza el código generado](/es-mx/guides/updates/).

## Para quién es

- Desarrolladores que crean los mismos tipos de archivos una y otra vez y quieren que encajen con el stack que ya usan.
- Equipos que guardan sus convenciones en un pack y quieren que las correcciones lleguen al código que ya se generó. Consulta [Escribe un pack](/es-mx/packs/write-a-pack/).
- Agentes de IA. Todos los comandos aceptan `--json` y `--no-input`, y [`cuilo mcp`](/es-mx/agents/mcp/) ofrece por MCP los mismos pasos de plan, revisión y aplicación.

## Qué sigue

- [Instala cuilo](/es-mx/install/).
- [Genera tu primera pieza](/es-mx/first-piece/) en un proyecto de SvelteKit. Luego edítala, inspecciónala y deshazla.
- [Packs y piezas](/es-mx/guides/packs-and-pieces/) cubre entradas, variantes y carpetas de salida.
- [Seguridad](/es-mx/security/) explica lo que los packs pueden y no pueden hacer.
