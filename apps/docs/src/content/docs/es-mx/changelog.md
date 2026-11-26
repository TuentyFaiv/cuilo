---

title: Registro de cambios
description: Qué hay de nuevo en cada versión de cuilo, empezando por 2.0.0-alpha.0, la reescritura que reemplaza a tfverse.
---

## 2.0.0-alpha.0 (sin publicar)

La primera versión de cuilo 2. Es una reescritura de tfverse, la CLI de v1, con otro nombre: el comando es `cuilo` y el paquete de npm es `@cuilo/cli`. Consulta [Migración desde v1](/es-mx/migrating-from-v1/).

### Packs y piezas

- El código viene de packs: conjuntos versionados con un `pack.toml` y una carpeta por pieza. Consulta [Escribe un pack](/es-mx/packs/write-a-pack/).
- Las plantillas son código real. Cada entrada nombra un ejemplo, y cuilo lo reemplaza en todos sus formatos de nombre, también en los nombres de archivo. El código opcional usa las directivas en comentarios `@cuilo if`, `@cuilo else` y `@cuilo end`.
- Las piezas declaran entradas tipadas una sola vez. La misma definición controla las preguntas en la terminal, los flags `-i key=value` y lo que `describe_piece` les dice a los agentes.
- Las piezas se adaptan al stack detectado con variantes y condiciones de stack como `default_when`. `cuilo detect` muestra lo que encontró.
- El JavaScript y TypeScript generado escribe los imports relativos como los resuelve el proyecto: `./foo` con bundlers, `./foo.js` con `NodeNext` o `Node16`, y `./foo.ts` en Deno o con `allowImportingTsExtensions` o `rewriteRelativeImportExtensions`. cuilo lee el `tsconfig.json` que rige la carpeta de cada pieza, con lo que extiende, y registra el estilo en el dato `import_style`, que `cuilo detect` muestra, `cuilo.json` puede definir y `cuilo.lock` conserva para las actualizaciones. Lee esas configuraciones a través de enlaces simbólicos y uniones de directorio (junctions) de Windows, como los enlaces de pnpm en `node_modules`, cuando el archivo real está dentro del proyecto. Las plantillas de los packs mantienen los imports sin extensión. Consulta [imports relativos en el código generado](/es-mx/guides/stack-detection/#imports-relativos-en-el-código-generado).
- Una pieza puede requerir otras piezas de su pack, listar los paquetes que importa su código y parchear archivos existentes con parches `anchor`, `import` y `css_vars`. El `file` de un parche puede listar alternativas, y se usa la primera que tiene el proyecto; un `anchor` también, y se usa la primera que contiene el archivo.
- Las piezas listan siguientes pasos después de `cuilo add`, como montar rutas nuevas, en lugar de indicaciones de configuración en el código generado. Con `--wire`, o con `wire = true` en la configuración de usuario, cuilo hace los pasos que puede como parches dentro del plan. Consulta [siguientes pasos y conexión](/es-mx/guides/packs-and-pieces/#siguientes-pasos-y-conexión).
- Orígenes de packs: packs oficiales integrados en el binario, carpetas, tags o commits con `gh:`, tarballs por HTTPS, `git:` por SSH, elementos de `registry:` y namespaces de la configuración del proyecto. Consulta [Orígenes de packs](/es-mx/packs/sources/).
- Dos packs oficiales vienen con cuilo: `ui` 0.2.0 (`atom`, `molecule`, `organism`) y `backend` 0.2.0 (`resource`, `error`, `middleware`, `database`).
- `cuilo adopt` toma código existente como una pieza. `cuilo pack capture` convierte archivos del proyecto en una pieza de un pack local. Consulta [Adopta y captura](/es-mx/guides/adopt-and-capture/).
- `cuilo pack list`, `cuilo pack show` y `cuilo pack add` listan, describen y cargan packs.

### Planes e historial

- Cada cambio es primero un plan. `--dry-run` lo muestra, `--diff` agrega los diffs y `--review` conserva u omite cada fragmento, como `git add -p`. No se escribe nada hasta que apliques.
- Cada aplicación es una transacción registrada en la bitácora `.cuilo/history`. `cuilo log` las lista, y `cuilo undo` revierte la última; se niega a descartar ediciones hechas desde entonces, salvo con `--force`. Consulta [Historial y deshacer](/es-mx/guides/history/).
- `cuilo.lock` registra los packs y los archivos generados. `cuilo status` muestra si cada archivo está limpio, modificado o si falta, y `cuilo why <path:line>` dice si una línea vino del pack o de ti.
- En proyectos de confianza, los archivos generados pasan al planear por el formateador y los linters con corrección automática del propio proyecto (ESLint, Biome, Prettier, oxfmt, dprint, Ruff, Black, rustfmt, gofmt). Consulta [Formateadores y confianza](/es-mx/guides/formatters-and-trust/).
- Los planes listan los paquetes que necesitan sus piezas. Después de aplicar, cuilo ofrece el comando para el gestor de paquetes del proyecto, y `--install` o `--no-install` deciden sin preguntar. Consulta [Paquetes que necesita una pieza](/es-mx/guides/dependencies/).
- `--verify` ejecuta las comprobaciones del propio proyecto en una copia desechable con el plan aplicado, y rechaza el plan cuando una comprobación falla por su culpa. Consulta [Verifica un plan](/es-mx/guides/verify/).

### Actualizaciones

- `cuilo update <pack>` mueve todas las piezas de un pack a otra versión en un solo plan y fusiona tus ediciones línea por línea.
- `--on-conflict` elige qué pasa donde tus ediciones y la versión nueva cambian las mismas líneas: `refuse` (por defecto), `markers`, `ours` o `theirs`.
- Los packs pueden incluir migraciones para código que no es suyo, como un `rename` integrado de palabras completas o como reglas de ast-grep. `--no-migrate` las omite.
- Un fragmento que omites en `--review` cuenta como edición tuya, así que las actualizaciones posteriores lo siguen omitiendo.

Consulta [Actualiza el código generado](/es-mx/guides/updates/).

### Seguridad

- Un pack solo escribe en las rutas que declaran sus globs de `permissions.write`, y nunca en `.git`, `.cuilo` ni `cuilo.lock`, con cualquier combinación de mayúsculas y minúsculas o con otro nombre para la misma entrada.
- Los planes marcan las rutas riesgosas, como `package.json`, los workflows de CI, la configuración del editor y las instrucciones para agentes.
- `--verify` arranca las comprobaciones solo después de que ves el plan y aceptas, o con `--yes`, y solo en un proyecto en el que confías. Las copias viven en una carpeta privada de tu caché.
- Confianza en el proyecto: las herramientas del propio proyecto solo se ejecutan después de que confías en él, con la pregunta que hace `cuilo add` o con `cuilo trust`. `cuilo trust --revoke` retira la confianza, también en las sesiones MCP que están corriendo.
- Los packs se pueden firmar con minisign. Puedes exigir firmas, confiar en llaves y revocarlas, y el lockfile fija la llave que firmó cada pack.
- El lockfile fija cada pack por el SHA-256 de sus archivos. Se rechaza un origen que sirve otros archivos para la misma referencia fijada.
- Los nombres oficiales `ui` y `backend` están reservados para los packs oficiales.
- `cuilo undo` rechaza una entrada de la bitácora que nombra una ruta en la que cuilo nunca escribe, así que un proyecto clonado no puede sembrar una.
- El texto de packs y proyectos se escapa antes de llegar a la terminal.
- Las credenciales solo vienen de tu configuración de usuario y solo van a su propio host.

Consulta [Seguridad](/es-mx/security/).

### Agentes

- `cuilo mcp` sirve el flujo de plan, revisión y aplicación por MCP sobre stdio, con 18 herramientas. No se escribe nada antes de `apply_plan`.
- Las anotaciones de las herramientas marcan `apply_plan`, `undo` y `verify_plan` como no de solo lectura, así que los clientes pueden preguntarte primero.
- Todos los comandos aceptan `--json` y `--no-input`. Los códigos de salida son `0` terminado, `1` falló, `2` error de uso o falta confirmar, y `3` plan rechazado. Consulta [Códigos de salida](/es-mx/reference/exit-codes/).
- El sitio de documentación sirve `/llms.txt` y `/llms-full.txt`. Consulta [Dale la documentación a un agente](/es-mx/agents/llms-txt/).

Consulta [Conecta un agente por MCP](/es-mx/agents/mcp/).

### Distribución

Estos canales abren con la primera versión.

- Un solo conjunto de binarios para todos los canales, compilados por GitHub Actions con su atestación y firmados con minisign.
- npm (`@cuilo/cli`), sin dependencias y sin nada que se ejecute al instalar; el gestor de paquetes elige el paquete de tu plataforma.
- Scripts de instalación para macOS, Linux y Windows que verifican el SHA-256, y la firma cuando `minisign` está instalado. `CUILO_REQUIRE_SIGNATURE=1` hace obligatoria la firma.
- Homebrew (`tuentyfaiv/tap/cuilo`) y cargo (`cargo binstall cuilo`, `cargo install cuilo --locked`).
- `cuilo upgrade` reemplaza una copia instalada con el script por una versión firmada más nueva, y corre npm, pnpm, Bun, Yarn, Homebrew o cargo para las copias que ellos instalaron de forma global. Para una copia de `npx`, `bunx`, `pnpm dlx` o `yarn dlx`, o una en el `node_modules` de un proyecto, no corre nada e imprime el comando que debes correr. `cuilo upgrade --check` solo informa qué hay disponible.
- En una terminal, cuilo busca una versión nueva una vez al día en segundo plano. Las copias instaladas con el script se actualizan solas, las demás imprimen un aviso, y las copias de ejecutores de un solo uso no hacen ninguna de las dos cosas. `updates` en la configuración de usuario, o `CUILO_UPDATES`, elige `auto`, `notify` u `off`; cuilo advierte sobre cualquier otro valor de `CUILO_UPDATES` y lo ignora.
- `cuilo completions <bash|zsh|fish|powershell|elvish>` imprime un script de autocompletado para tu shell.

Consulta [Instalación](/es-mx/install/).

### Límites conocidos

- Todavía no se publica nada. Compila cuilo desde el código hasta que la primera versión abra los canales de instalación.
- Ningún comando crea un proyecto nuevo. cuilo agrega código a uno existente.
- Muchos tipos de v1, como páginas, layouts, hooks y stores, no tienen pieza oficial. Consulta [Migración desde v1](/es-mx/migrating-from-v1/).
- Los elementos de `registry:` todavía no aplican sus campos `css`, `tailwind` y `envVars`. La descripción del pack los menciona.
- Los orígenes `git:` solo funcionan por SSH.
- Los cambios que hace un gestor de paquetes quedan fuera del historial de cuilo, así que `cuilo undo` no los revierte.
- La copia de `--verify` no es un sandbox: las comprobaciones se ejecutan como tú. También rechaza proyectos con más de 50,000 archivos o 1 GB fuera de las carpetas de build y de dependencias.
- Los planes MCP pendientes y los packs cargados con `add_pack` solo duran lo que dura la sesión.
- Los binarios de Windows y macOS no tienen firma de código con Authenticode ni notarización de Apple.
