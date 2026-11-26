---

title: Seguridad
description: Cómo limita cuilo lo que hacen los packs, los proyectos y los agentes, qué revisa antes de escribir o ejecutar algo y cómo reportar una vulnerabilidad.
---

cuilo ejecuta código y escribe archivos por ti, con packs escritos por otras personas. Esta página explica qué revisa, qué nunca hace y dónde están sus límites.

## Primero el plan

Cada comando que cambia el proyecto arma primero un plan. No se escribe nada hasta que lo apliques.

- Las plantillas no pueden leer archivos ni el entorno, ni ejecutar código.
- Los formateadores se ejecutan al planear, sobre el texto de cada archivo por stdin, así que no se escribe nada antes de que apliques.
- `--dry-run` muestra el plan y se detiene. En una terminal, cuilo pregunta antes de aplicar, salvo que pases `--yes`. Con `--no-input` o `--json` y sin `--yes`, no se aplica nada y el código de salida es `2`.
- Cada aplicación es una sola transacción, registrada en la bitácora `.cuilo/history`. Si un archivo cambió después de crear el plan, la aplicación se detiene y no se escribe nada; vuelve a planear.
- Solo un proceso de cuilo cambia un proyecto a la vez. Mientras trabaja, mantiene `.cuilo/busy`.
- cuilo nunca lee ni escribe a través de enlaces simbólicos. Las rutas no pueden salir del proyecto: se rechazan las rutas absolutas, los segmentos `..` y las barras invertidas.

Los agentes siguen la misma regla por MCP: solo `apply_plan` escribe un plan. Consulta [Conecta un agente por MCP](/es-mx/agents/mcp/).

## Qué puede escribir un pack

Un pack declara las rutas en las que puede escribir como globs en `permissions.write` de su `pack.toml`. Un plan que escribe en cualquier otro lugar se rechaza con el código de salida `3`. Un pack que lista paquetes para que el proyecto los instale también necesita `permissions.dependencies = true`. Consulta el [manifiesto del pack](/es-mx/reference/pack-manifest/).

Hay rutas en las que ningún pack puede escribir, aunque las declare:

- `.git`, la carpeta de Git del proyecto
- `.cuilo`, el estado propio de cuilo
- `cuilo.lock`, el lockfile

Siguen prohibidas con cualquier combinación de mayúsculas y minúsculas, como `.GIT/config` o `Cuilo.LOCK`, y con otro nombre que el disco resuelva a la misma entrada, como un nombre corto de Windows tipo `GIT~1` o un enlace duro al lockfile.

## Rutas riesgosas

Algunos archivos se ejecutan como código, cambian cómo se compila el proyecto o dan instrucciones a los agentes de IA. Cada plan marca un cambio en uno de ellos y dice por qué, sin importar mayúsculas o minúsculas. La marca no bloquea el plan. Te pide que leas ese cambio con cuidado.

| Ruta                                                                     | Por qué el plan la marca                                         |
| ------------------------------------------------------------------------ | ---------------------------------------------------------------- |
| `**/package.json`                                                        | los scripts del paquete se ejecutan al instalar                  |
| `**/.npmrc`                                                              | cambia cómo se instalan los paquetes                             |
| `**/*.config.js`, `**/*.config.ts`, `**/*.config.mjs`, `**/*.config.cjs` | la configuración de build se ejecuta como código                 |
| `**/build.rs`                                                            | los scripts de build de Cargo se ejecutan como código            |
| `**/Cargo.toml`                                                          | cambia las dependencias y el build de Rust                       |
| `**/pyproject.toml`                                                      | cambia las dependencias y el build de Python                     |
| `**/Makefile`                                                            | las recetas de build se ejecutan como código                     |
| `**/Dockerfile`                                                          | el build del contenedor se ejecuta como código                   |
| `.github/**`                                                             | los workflows de CI se ejecutan con los secretos del repositorio |
| `.gitlab-ci.yml`                                                         | los pipelines de CI se ejecutan con los secretos del repositorio |
| `.husky/**`                                                              | los hooks de git se ejecutan al hacer commit                     |
| `.vscode/**`                                                             | las tareas y la configuración del editor pueden ejecutar código  |
| `.idea/**`                                                               | la configuración del editor puede ejecutar código                |
| `.claude/**`, `.cursor/**`, `.codex/**`                                  | configuración de agentes de IA                                   |
| `**/AGENTS.md`, `**/CLAUDE.md`                                           | instrucciones para agentes de IA                                 |
| `**/.env*`                                                               | puede contener secretos                                          |

En la salida de `--json` y por MCP, cada cambio marcado trae esta razón en su campo `risk`.

## `--verify` y la copia privada

`--verify` aplica el plan a una copia desechable del proyecto y ahí ejecuta las comprobaciones del propio proyecto. La [guía de verificación](/es-mx/guides/verify/) explica qué comprobaciones se ejecutan.

Las comprobaciones se ejecutan con los archivos del plan en su lugar, así que el código del pack, como un `build.rs` o una prueba, se ejecuta como tú. Por eso:

- cuilo imprime el plan primero y arranca las comprobaciones solo después de que aceptes, o con `--yes`. Esto también aplica con `--dry-run`. Sin terminal y sin `--yes`, no se ejecuta nada y el código de salida es `2`.
- `--verify` necesita un proyecto en el que confíes, porque las comprobaciones son código del propio proyecto.
- Por MCP, `verify_plan` está marcada como no de solo lectura, así que los clientes que respetan las anotaciones te preguntan primero.

La copia se mantiene privada:

- Las copias viven en la carpeta `verify` de tu caché de cuilo. En Unix, solo tú puedes abrir cada copia.
- cuilo rechaza una carpeta de caché que otros usuarios pueden abrir, y también una que esté dentro de una carpeta en la que todos pueden escribir, como `/tmp`. Las herramientas de las comprobaciones, como Cargo, leen archivos de configuración de las carpetas superiores. Si lo necesitas, define `CUILO_CACHE_DIR` con una carpeta tuya.
- La copia deja fuera `.git`, `.cuilo`, la salida del build y las cachés. `node_modules`, `.venv` y `venv` se enlazan en vez de copiarse, así que se rechaza un plan que escriba dentro de ellas.
- Cada copia se borra cuando termina la verificación. Si una ejecución interrumpida deja una copia, una verificación posterior la borra cuando cumple 24 horas.

:::caution
La copia no es un sandbox. Las comprobaciones se ejecutan como tú, con tus variables de entorno y acceso a la red. cuilo detiene una comprobación después de 10 minutos. Usa `--verify` solo con packs a los que dejarías ejecutar código en tu máquina.
:::

## Confianza en el proyecto

Los formateadores y los linters con corrección automática que vienen del proyecto, o cuya configuración puede ejecutar código, solo se ejecutan en proyectos en los que confías. Eso incluye ESLint, Biome, Prettier, oxfmt, dprint, Ruff y Black, además de las comprobaciones de `--verify`. rustfmt y gofmt también necesitan confianza: vienen de tu sistema, pero los archivos del proyecto pueden elegir cuál se ejecuta, como hace `rust-toolchain.toml` con rustup.

- `cuilo add` pregunta una vez por proyecto y recuerda tu respuesta.
- `cuilo trust` confía en el proyecto sin preguntar. `cuilo trust --revoke` deja de confiar en él y recuerda esa respuesta. Las ejecuciones con `--no-input` o `--json` y las sesiones MCP nunca preguntan, así que dependen de este comando.
- Las respuestas se guardan en `trusted-projects.toml`, junto a tu configuración de usuario, según la ruta completa del proyecto. Viven fuera del proyecto, así que un proyecto clonado no puede confiar en sí mismo.
- `[tools] trust = "always"` o `"never"` en tu [configuración de usuario](/es-mx/reference/user-config/) evita la pregunta en todos lados.
- La pregunta solo nombra formateadores que cuilo conoce, así que no muestra ningún texto escrito por el proyecto.
- Mientras no confíes en un proyecto, los planes dicen qué archivos no se formatearon.
- Revocar la confianza también llega a las sesiones de `cuilo mcp` que ya están corriendo: en su siguiente llamada, dejan de ejecutar las herramientas del proyecto.

La [guía de formateadores y confianza](/es-mx/guides/formatters-and-trust/) tiene los detalles.

## Firmas y firmantes fijados

Un pack se puede firmar con [minisign](https://jedisct1.github.io/minisign/). La firma cubre el mensaje de una línea `cuilo pack <name> <version> <digest>` y vive en `pack.minisig`, en la raíz del pack. Tú eliges en qué llaves confiar, en la tabla `[signing]` de tu configuración de usuario:

```toml
[signing]
require = true                 # refuse unsigned packs and packs signed by keys you have not trusted
keys = { acme = "RWQ…" }       # minisign public keys
revoked = ["1C3B7AE36BE79191"] # key ids refused even when the signature is valid
```

- Esto siempre se rechaza: un `pack.minisig` que no es una firma de minisign, una firma hecha con una llave revocada y una firma hecha con una llave de confianza que no coincide con los archivos del pack. Las llaves revocadas vienen de tu lista `revoked` y de una lista integrada en cuilo.
- Un pack sin firma, o firmado con una llave en la que no has confiado, solo se carga cuando `require` está desactivado. En ese caso cuenta como sin firma.
- Dos llaves de confianza con el mismo id son un error de configuración.
- El lockfile registra la llave pública completa que verificó cada pack, no solo su id. Se rechaza una versión posterior firmada con otra llave, o sin firma.
- Los packs oficiales vienen dentro de cuilo y no necesitan firma.

Consulta [Firma un pack](/es-mx/packs/signing/).

## Hashes del lockfile

`cuilo.lock` registra el origen de cada pack y el hash SHA-256 de sus archivos. El archivo de firma no forma parte del hash.

- Un pack fijado se carga desde la caché, sin conexión.
- Se rechaza un origen que ahora sirve otros archivos para la misma referencia fijada.
- Se rechaza un pack cuyos archivos no coinciden con los fijados para la misma versión.
- Un registro del lockfile solo carga el pack y la versión que nombra.

Los packs oficiales son la excepción: vienen con cuilo, así que una versión más nueva de cuilo trae su siguiente versión. Consulta la [referencia del lockfile](/es-mx/reference/lockfile/).

## Nombres oficiales reservados

Los nombres de los packs oficiales, `ui` y `backend`, están reservados. Se rechaza un pack de cualquier otro origen que use uno de ellos, así que una descarga no puede hacerse pasar por un pack oficial.

## Descargas y credenciales

- Las descargas solo usan HTTPS y se validan contra los certificados de confianza del sistema operativo. Una descarga sigue como máximo 5 redirecciones, y solo a HTTPS.
- Los archivos comprimidos se leen en memoria y nunca se extraen en disco. Solo se leen archivos regulares, así que se omiten los enlaces que haya dentro. Un archivo comprimido puede tener hasta 20,000 entradas y 64 MB de archivos.
- Las credenciales vienen de tu configuración de usuario, nunca de la configuración del proyecto, así que un proyecto clonado no puede mandar tus tokens a otro lado. La configuración solo guarda el nombre de la variable de entorno que contiene el token.
- Un token solo va a su propio host, nunca a un host al que redirija una descarga.
- Los orígenes `git:` solo funcionan por SSH y ejecutan tu propio `git`. cuilo nunca te pide nada en pantalla (SSH corre en modo batch, salvo que definas tu propio comando de SSH), detiene git después de dos minutos y usa `git archive`, así que no hace checkout de nada.

Consulta [Orígenes de packs](/es-mx/packs/sources/).

## Deshacer y bitácoras sembradas

La bitácora en `.cuilo/history` es estado del proyecto, y un proyecto clonado puede traer una. `cuilo undo` revisa la última entrada antes de escribir algo:

- Rechaza una entrada que nombra `.git`, `.cuilo` o `cuilo.lock`, con cualquier combinación de mayúsculas y minúsculas o con otro nombre para la misma entrada. cuilo nunca escribe en esas rutas, así que esa entrada no la registró cuilo. `--force` no cambia esto. El error te dice que borres la entrada de `.cuilo/history` si no la creaste tú.
- Se niega cuando un archivo no tiene ni lo que dejó la transacción ni, si la transacción se interrumpió, lo que encontró. `--force` pasa por encima de esto, y se pierden las ediciones hechas desde entonces.

Consulta la [guía del historial](/es-mx/guides/history/).

## Salida en la terminal

El texto de packs, proyectos y otros programas se escapa antes de que cuilo lo imprima. Los caracteres de control, salvo los saltos de línea y los tabuladores, y los caracteres que reordenan el texto (U+202A a U+202E y U+2066 a U+2069) se muestran como secuencias de escape, como `\u{1b}`. Un pack o un proyecto no puede mover el cursor, borrar líneas ni reescribir lo que muestra tu terminal. El texto que debe quedar en una sola línea también muestra los saltos de línea como `\n`.

## Integridad de la instalación

Todavía no hay versiones publicadas. Cuando las haya, todos los canales instalarán los mismos binarios, compilados por GitHub Actions con su atestación:

| Canal                                | Qué lo verifica                                                     |
| ------------------------------------ | ------------------------------------------------------------------- |
| npm, pnpm, bun, yarn                 | Integridad del lockfile, procedencia de npm (provenance)            |
| Script de instalación (macOS, Linux) | SHA-256, y la firma cuando `minisign` está instalado                |
| Script de instalación (Windows)      | SHA-256, y la firma cuando `minisign` está instalado                |
| Homebrew                             | El SHA-256 de la fórmula                                            |
| cargo                                | La firma (`cargo binstall`), las sumas de verificación de crates.io |

- El paquete de npm no tiene dependencias y no ejecuta nada al instalarse.
- Los scripts de instalación solo descargan por HTTPS y no siguen redirecciones. `install.sh` además exige TLS 1.2 o más reciente. Con `CUILO_REQUIRE_SIGNATURE=1`, se niegan a instalar a menos que `minisign` verifique la firma.
- `cuilo upgrade` y las actualizaciones automáticas solo reemplazan copias que instaló el script de instalación. Reemplazan el binario solo cuando el `SHA256SUMS` de la versión trae una firma válida de la llave de publicación integrada en cuilo, y el binario coincide con su checksum firmado. Las actualizaciones automáticas nunca bajan a una versión anterior, y `cuilo upgrade` solo lo hace con `--version`.
- Para otras copias, `cuilo upgrade` corre el gestor de paquetes que las instaló, como un programa con argumentos fijos y nunca a través de un shell, y ese gestor de paquetes revisa la descarga. Las actualizaciones automáticas nunca corren un gestor de paquetes.
- La búsqueda diaria de actualizaciones solo lee cuál es la versión más reciente. `updates = "off"` en tu configuración de usuario, o `CUILO_UPDATES=off`, la desactiva.
- Cada firma nombra su versión y su archivo, así que no se puede reutilizar para otro archivo u otra versión.
- Para ver dónde se compiló un binario: `gh attestation verify $(command -v cuilo) --repo TuentyFaiv/cuilo`.

Cada archivo de una versión publicada se firma con esta llave de minisign:

```text
RWTFcgVmt+E7Xi77vi6Q/Yu68TJDTsBTKYHqOmyjSOY4VCgpEFCwl+dK
```

:::note[El límite]
Las sumas de verificación vienen del mismo host que los binarios, así que por sí solas solo detectan una descarga corrupta. La firma y la atestación son las que demuestran que una versión salió del workflow de publicación de cuilo. El primer `curl … | sh` confía en el host de descarga por HTTPS, como hacen rustup, Bun y Deno. Si quieres ir más allá, descarga `install.sh` junto con `install.sh.minisig`, verifícalos con `minisign -V -P <key> -m install.sh` y ejecuta el script con `CUILO_REQUIRE_SIGNATURE=1`. Los binarios de Windows y macOS todavía no tienen firma de código con Authenticode ni notarización de Apple.
:::

Consulta [Instalación](/es-mx/install/).

## Anotaciones de las herramientas MCP

Cada herramienta MCP trae anotaciones que le dicen al cliente qué hace. Las herramientas de solo lectura tienen `readOnlyHint: true`. `apply_plan` y `undo` escriben en el proyecto, y `verify_plan` ejecuta código del pack, así que las tres tienen `readOnlyHint: false` y `destructiveHint: true`. `verify_plan` no cambia nada en el proyecto, pero se marca así para que los clientes te pregunten antes de ejecutarla. `add_pack` y `plan_update` tienen `openWorldHint: true` porque pueden descargar packs.

Las anotaciones y las instrucciones del servidor guían a los agentes y a los clientes. No pueden detener a un agente que las ignora, así que deja activadas las solicitudes de aprobación de tu cliente. Consulta [Conecta un agente por MCP](/es-mx/agents/mcp/).

## Reporta una vulnerabilidad

Reporta las vulnerabilidades en privado con el formulario de avisos de seguridad de GitHub: [github.com/TuentyFaiv/cuilo/security/advisories/new](https://github.com/TuentyFaiv/cuilo/security/advisories/new). No abras un issue público. Incluye la versión de cuilo (`cuilo --version`), tu sistema operativo y los pasos o el pack que muestran el problema.
