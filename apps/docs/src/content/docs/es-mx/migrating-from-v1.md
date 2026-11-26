---

title: Migración desde v1
description: Qué era tfverse (v1), qué cambió en cuilo (v2), cómo cambiar uno por el otro y qué comandos viejos tienen un equivalente nuevo.
---

cuilo 2 reemplaza a tfverse, la CLI de v1. Es un programa nuevo, con otro nombre, otro paquete y otra forma de generar código. La mayoría de las opciones de v1 no tienen un equivalente directo, y esta página lo indica en cada caso.

:::caution[cuilo 2 está en alfa]
Todavía no se publica nada de v2. Hasta la primera versión, [compila cuilo desde el código](/es-mx/install/). Mientras tanto, tu instalación de v1 sigue funcionando, y puedes tener las dos instaladas al mismo tiempo porque los comandos se llaman distinto.
:::

## Qué era v1

- **Nombre y paquete**: el comando era `tfverse`, publicado en npm como `@tuentyfaiv/cli`. La última versión fue la 0.0.11.
- **Instalación**: un script `postinstall` de npm descargaba el binario de tu plataforma desde un bucket de almacenamiento, sin verificar ningún checksum.
- **Qué generaba**: código base para proyectos de React, Svelte y JavaScript vanilla. Ofrecía componentes (átomos, moléculas, organismos o personalizados), páginas, layouts, servicios y esquemas, además de HOCs, hooks y contextos de React, actions y stores de Svelte, y clases de vanilla. No todos los tipos estaban listos para todas las herramientas.
- **Proyectos**: con `project` o `library`, clonaba un repositorio boilerplate, reiniciaba su historial de Git y hacía un primer commit.
- **Flujo**: hacía preguntas o recibía flags, preguntaba "Are you sure?" y luego escribía los archivos. No mostraba una vista previa del contenido, no guardaba registro de lo que generaba y no tenía forma de deshacer ni de actualizar.
- **Personalización**: una configuración JSON en `~/.tfverse/config_cli.json` (guardada con `tfverse -g`), o un archivo que pasabas con `-c`. Definía rutas, repositorios boilerplate y carpetas de plantillas personalizadas que usaban palabras clave como `NAME_PASCAL`.

## Qué cambió en v2

|                            | v1 (tfverse)                                                                           | v2 (cuilo)                                                                                                                                              |
| -------------------------- | -------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Comando                    | `tfverse`                                                                              | `cuilo`                                                                                                                                                 |
| Paquete de npm             | `@tuentyfaiv/cli`                                                                      | `@cuilo/cli`                                                                                                                                            |
| Paso de instalación en npm | `postinstall` descarga el binario                                                      | Sin dependencias, no se ejecuta nada al instalar                                                                                                        |
| Otros canales              | ninguno                                                                                | Scripts de instalación, Homebrew, cargo (consulta [Instalación](/es-mx/install/))                                                                       |
| De dónde viene el código   | Plantillas integradas y carpetas de plantillas                                         | [Packs](/es-mx/packs/write-a-pack/) versionados del set oficial, una carpeta, GitHub, un tarball por HTTPS, `git:` por SSH o un elemento de un registry |
| Stacks                     | React, Svelte, JavaScript vanilla                                                      | Se detectan desde el proyecto; los packs oficiales cubren frameworks web, UI en Rust, Flutter, Jetpack Compose, SwiftUI y seis frameworks de backend    |
| Antes de escribir          | "Are you sure?"                                                                        | Un plan de cada archivo, con `--dry-run`, `--diff` y `--review`                                                                                         |
| Después de escribir        | No se registra nada                                                                    | Una transacción que puedes deshacer y un lockfile que rastrea los archivos generados                                                                    |
| Actualizaciones de packs   | ninguna                                                                                | `cuilo update` fusiona la versión nueva con los archivos que editaste                                                                                   |
| Scripts y agentes          | Flags para algunas respuestas; otras, como el tipo de componente, siempre se preguntan | `--json` y `--no-input` en todos los comandos, y `cuilo mcp` para agentes de IA                                                                         |
| Creación de proyectos      | Clona un repositorio boilerplate                                                       | No disponible: cuilo agrega código a un proyecto existente                                                                                              |

## Desinstala v1

Desinstala el paquete con el mismo gestor de paquetes con el que lo instalaste. Ejecuta la línea que te corresponda:

```sh
npm uninstall --global @tuentyfaiv/cli   # npm
pnpm remove --global @tuentyfaiv/cli     # pnpm
yarn global remove @tuentyfaiv/cli       # Yarn 1
bun remove --global @tuentyfaiv/cli      # Bun
```

El binario vivía dentro de la carpeta del paquete, así que esto también lo quita. Comprueba que el comando ya no exista:

```sh
command -v tfverse   # prints nothing once it is removed
```

Si ejecutaste `tfverse -g`, v1 guardó su configuración en `~/.tfverse`. Borra esa carpeta cuando ya no la necesites:

```sh
rm -rf ~/.tfverse
```

## Instala v2

Hasta la primera versión, compila cuilo desde un clon del repositorio:

```sh
cargo install --path crates/cuilo --locked   # puts `cuilo` in ~/.cargo/bin
cuilo --version
```

Cuando haya versiones publicadas, también funcionarán `npm install --global @cuilo/cli` y los demás canales de [Instalación](/es-mx/install/).

## Comandos viejos y nuevos

v1 recibía un solo comando con flags y preguntaba el resto. v2 tiene un comando por tarea. La pieza que generas se nombra como `pack/piece`, y el stack viene del proyecto en lugar de flags.

### Qué generar

| v1                                        | v2                                                                                                                                                                  |
| ----------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `tfverse -a component`, tipo `atoms`      | `cuilo add ui/atom <name>`                                                                                                                                          |
| `tfverse -a component`, tipo `molecules`  | `cuilo add ui/molecule <name>`                                                                                                                                      |
| `tfverse -a component`, tipo `organisms`  | `cuilo add ui/organism <name>`                                                                                                                                      |
| `tfverse -a component`, tipo `custom`     | No hay pieza oficial. [Escribe un pack](/es-mx/packs/write-a-pack/), o convierte un componente que ya tengas en una pieza con `cuilo pack capture`.                 |
| `tfverse -a project`, `-a library`        | Sin equivalente. cuilo no crea proyectos.                                                                                                                           |
| `tfverse -a page`, `-a layout`            | Sin equivalente.                                                                                                                                                    |
| `tfverse -a service`, `-a schema`         | Sin equivalente para código de front-end. `backend/resource` genera un recurso REST del lado del servidor (modelo, repositorio, servicio, rutas), que es otra cosa. |
| `tfverse -a hoc`, `-a hook`, `-a context` | Sin equivalente.                                                                                                                                                    |
| `tfverse -a action`, `-a store`           | Sin equivalente.                                                                                                                                                    |
| `tfverse -a class`                        | Sin equivalente.                                                                                                                                                    |

Ejecuta `cuilo pack list` para ver todas las piezas, y `cuilo pack show ui` para ver las piezas de un pack y sus entradas.

### Flags

| v1                            | v2                                                                                                                                                                                                                                                         |
| ----------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `[NAME]`                      | `[name]`, la entrada `name` de la pieza, por ejemplo `cuilo add ui/atom status-badge`. Las demás entradas usan `-i key=value`.                                                                                                                             |
| `-t, --tool`                  | Se detecta desde el proyecto. `cuilo detect` muestra lo que encontró, `--variant` elige otra variante y `stack` en `cuilo.json` reemplaza la detección.                                                                                                    |
| `-l, --language`              | Se detecta desde el proyecto, o se define con `stack` en `cuilo.json`.                                                                                                                                                                                     |
| `-s, --styles`                | Sin flag. Las piezas de `ui` escriben CSS modules, o clases de Tailwind cuando el proyecto usa Tailwind.                                                                                                                                                   |
| `-p, --path`                  | `--target <folder>`. La carpeta debe ser una en la que el pack pueda escribir; el pack `ui` escribe en `src/ui/`, entre otras.                                                                                                                             |
| `-y, --sure`                  | `-y, --yes` aplica sin preguntar.                                                                                                                                                                                                                          |
| `-g, --global`                | No hay un cuestionario equivalente. `defaults` en `cuilo.json` define valores de entrada por pieza. Tu [configuración de usuario](/es-mx/reference/user-config/) guarda las credenciales, las llaves de firma de confianza y la confianza en herramientas. |
| `-c, --config <FILE>`         | Sin flag. cuilo lee `cuilo.json` o `cuilo.toml` en la raíz del proyecto; `--root` elige el proyecto.                                                                                                                                                       |
| `-h, --help`, `-V, --version` | Igual que antes.                                                                                                                                                                                                                                           |

### Configuración

| Clave de configuración de v1 | v2                                                                                                                                                                                   |
| ---------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `paths`                      | `paths` en `cuilo.json`, con el nombre de la pieza como clave, por ejemplo `{ "paths": { "atom": "src/ui/components" } }`. La carpeta debe ser una en la que el pack pueda escribir. |
| `templates`                  | Un pack. Sus plantillas son código real, y el nombre de ejemplo se reemplaza en todos sus formatos de nombre, sin palabras clave como `NAME_PASCAL`.                                 |
| `tools_type`, `repository`   | Sin equivalente.                                                                                                                                                                     |
| `lang`, `styles`, `i18n`     | Sin clave de configuración. El lenguaje viene de la detección o de `stack`, y los estilos siguen al proyecto, como explican los flags de arriba.                                     |

Consulta [la referencia de configuración del proyecto](/es-mx/reference/configuration/) para ver cada campo de `cuilo.json`.

### Novedades de v2

Esto no tiene equivalente en v1: `cuilo status`, `cuilo why`, `cuilo log`, `cuilo undo`, `cuilo update`, `cuilo adopt`, `cuilo pack`, `cuilo trust`, `cuilo mcp`, y los flags `--dry-run`, `--diff`, `--review`, `--verify`, `--json` y `--no-input`. La [referencia de la CLI](/es-mx/reference/cli/) los lista todos.

## Código que generó v1

Los archivos que escribió v1 son código normal en tu proyecto. cuilo no sabe de ellos, y puedes dejarlos tal como están.

Para que cuilo se encargue de un archivo, `cuilo adopt` toma los archivos que están en las rutas donde escribiría una pieza, para que las actualizaciones posteriores se fusionen con ellos. Nunca cambia archivos existentes y crea los que faltan. v1 usaba otros nombres de archivo y otras carpetas, así que ejecútalo primero con `--dry-run` para ver qué archivos coinciden:

```sh
cuilo adopt ui/atom status-badge --dry-run
```

Para reutilizar una plantilla de v1, genera un componente con ella y luego captura ese componente como pieza de un pack local. El nombre que pasas con `--name` debe tener dos palabras o más:

```sh
cuilo pack capture src/ui/atoms/user-card/user-card.tsx --piece card --name UserCard
```

[Adopta y captura](/es-mx/guides/adopt-and-capture/) explica los dos comandos.
