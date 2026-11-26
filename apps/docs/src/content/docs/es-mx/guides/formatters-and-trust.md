---

title: Formateadores y confianza
description: cuilo ejecuta el formateador y los linters de tu proyecto sobre los archivos generados, pero solo en proyectos en los que confías con cuilo trust.
---

Los archivos generados siguen el formateador y los linters que corrigen de tu proyecto, así que pasan sus revisiones como si tú los hubieras escrito. cuilo nunca instala estas herramientas. Usa la versión que tu proyecto ya tiene instalada, y solo en un proyecto en el que confías.

## Qué herramientas se ejecutan

| Herramienta                                   | Se detecta por                                            | Se ejecuta                         |
| --------------------------------------------- | --------------------------------------------------------- | ---------------------------------- |
| ESLint (correcciones), Biome, Prettier, oxfmt | su paquete en `package.json` o su configuración           | desde `node_modules/.bin`          |
| dprint                                        | `dprint.json`                                             | `node_modules/.bin`, o si no, PATH |
| Ruff (correcciones y luego formato), Black    | `[tool.ruff]` / `[tool.black]`, `ruff.toml`, dependencias | desde `.venv`, o si no, PATH       |
| rustfmt, gofmt                                | un proyecto de Rust o Go                                  | desde PATH, con `rustfmt.toml`     |

Se ejecutan en este orden: ESLint, Ruff, Biome, Prettier, oxfmt, dprint, Black, rustfmt, gofmt. Los linters que corrigen van primero, así que el formateador tiene la última palabra. Cada herramienta recibe solo los tipos de archivo que maneja: Ruff y Black reciben archivos de Python, rustfmt recibe `.rs`, gofmt recibe `.go` y las herramientas de JavaScript reciben scripts y los demás tipos que formatean, como CSS o JSON.

cuilo las ejecuta desde la raíz del proyecto mientras arma el plan, sobre cada archivo a través de stdin, así que no se escribe nada antes de que apliques. `--dry-run` ya muestra los archivos formateados. El archivo formateado también es el original con el que se fusionan las [actualizaciones](/es-mx/guides/updates/) posteriores, así que `cuilo status` lo muestra como `clean`.

Para elegir tú las herramientas, define `stack.formatter` en `cuilo.json`, por ejemplo cuando el proyecto tiene varias. Usa `[]` para no usar ninguna. [`cuilo detect`](/es-mx/reference/cli/detect/) muestra lo que encontró cuilo.

```json
{
	"stack": { "formatter": ["eslint", "prettier"] }
}
```

## Confía en un proyecto

Los formateadores vienen del proyecto, y sus archivos de configuración pueden ejecutar código. Por eso cuilo solo los ejecuta en proyectos en los que confías. Mientras no confíes, el plan dice qué no formateó:

```text
Notes:
  cuilo.lock: not formatted with Prettier: cuilo runs a project's formatters and linters only once you trust it; run `cuilo trust` here
```

En una terminal, `cuilo add`, `cuilo adopt` y `cuilo update` preguntan una vez por proyecto, cuando el proyecto usa un formateador que cuilo conoce:

```text
This project uses Prettier. Let cuilo run them on the files it generates? They come from the project and run as you; your answer is remembered (`cuilo trust` changes it). [y/N]
```

cuilo recuerda tu respuesta, sea sí o no. [`cuilo trust`](/es-mx/reference/cli/trust/) la fija sin preguntar:

```sh
cuilo trust
```

```text
Trusted. cuilo will run Prettier on the files it generates here.
```

`--no-input`, `--json` y las sesiones de MCP nunca preguntan, así que dependen de `cuilo trust`. Esa misma confianza permite que [`--verify`](/es-mx/guides/verify/) ejecute las revisiones del proyecto.

cuilo guarda las respuestas en `trusted-projects.toml`, junto a tu [configuración de usuario](/es-mx/reference/user-config/): en `$CUILO_CONFIG_DIR`, o si no, en `$XDG_CONFIG_HOME/cuilo` o `~/.config/cuilo`, o en `%APPDATA%\cuilo` en Windows. Los proyectos se guardan por su ruta completa, así que si mueves un proyecto a otra carpeta, cuilo vuelve a preguntar.

### rustfmt y gofmt

rustfmt y gofmt vienen de tu sistema, no del proyecto, pero también necesitan confianza. Los archivos del proyecto pueden elegir qué versión se ejecuta, como hace `rust-toolchain.toml` con rustup. rustfmt también lee el `rustfmt.toml` del proyecto y toma la edición de `Cargo.toml`.

### Defínelo para todos los proyectos

`[tools] trust` en tu configuración de usuario evita la pregunta en todos lados:

```toml
[tools]
trust = "always" # or "never"; the default is "ask"
```

Este ajuste tiene prioridad. Con `"never"`, no se ejecutan las herramientas de ningún proyecto, ni siquiera después de `cuilo trust`.

## Revoca la confianza

```sh
cuilo trust --revoke
```

```text
cuilo will no longer run this project's own tools.
```

Revocarla también afecta a las sesiones de `cuilo mcp` y `cuilo rpc` que ya están en ejecución. Vuelven a revisar la confianza en cada llamada, así que dejan de ejecutar las herramientas del proyecto sin que tengas que reiniciarlas.

## Cuando una herramienta no se puede ejecutar

El plan anota cada problema, y los archivos se planean sin esa herramienta:

```text
Notes:
  cuilo.lock: Prettier is set up in this project but not installed, so generated files were not formatted with it; install the project's dependencies
```

```text
Notes:
  src/cards/profile-card.tsx: Prettier could not format this file: [error] src/cards/profile-card.tsx: SyntaxError: Unexpected token (3:1)
```

Un nombre en `stack.formatter` que cuilo no soporta también genera una nota:

```text
Notes:
  cuilo.lock: cuilo cannot run the formatter `stylua`; supported: eslint, ruff, biome, prettier, oxfmt, dprint, black, rustfmt, gofmt
```
