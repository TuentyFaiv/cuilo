---

title: Verifica un plan
description: Ejecuta las revisiones de tu proyecto en una copia con el plan aplicado, y aplica el plan solo si ninguna revisión falla por su culpa.
---

`--verify` prueba un plan antes de que toque tu proyecto. cuilo copia el proyecto, aplica el plan a la copia y ahí ejecuta las revisiones del propio proyecto. El plan se aplica solo si ninguna revisión falla por su culpa. Funciona con `cuilo add`, `cuilo adopt`, `cuilo update` y `cuilo pack capture`.

```sh
cuilo add ui/atom Button --verify
```

## Qué revisiones se ejecutan

Si `cuilo.json` tiene una lista `verify`, cuilo ejecuta exactamente esos comandos, en orden:

```json
{
	"verify": [["bun", "run", "check"]]
}
```

Si no, cuilo detecta las revisiones:

| Proyecto                  | Revisiones                                                                                                                                |
| ------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| Scripts de `package.json` | `typecheck`, `type-check`, `check`, `lint` y `test`, o si no hay, `build`, con `bun run`, `pnpm run`, `yarn run`, `deno task` o `npm run` |
| Rust                      | `cargo check --all-targets --quiet`                                                                                                       |
| Go                        | `go vet ./...`                                                                                                                            |
| Python con Ruff           | `ruff check .`                                                                                                                            |
| Python con pytest         | `uv run pytest -q` en un proyecto de uv, o si no, `pytest -q`                                                                             |

Ruff y pytest se ejecutan desde `.venv/bin` cuando el proyecto los tiene ahí. Cada revisión se ejecuta en la copia con `CI=1` y se detiene a los 10 minutos. Cargo compila en una carpeta de tu caché de cuilo, que se conserva entre ejecuciones para que las siguientes revisiones sean más rápidas.

Si no hay nada que ejecutar, cuilo se detiene:

```text
error: verify: no checks found: add scripts like `typecheck`, `lint` or `test`, or a `verify` list in the project config
```

## Cómo se juzga una falla

Una revisión que falla con el plan se vuelve a ejecutar en una copia nueva sin el plan. Así cuilo sabe si el plan causó la falla. Los resultados se imprimen bajo `Checks on a copy with the plan applied:`, una línea por revisión con lo que tardó:

| Etiqueta | Significado                                                                                                |
| -------- | ---------------------------------------------------------------------------------------------------------- |
| `pass`   | La revisión pasó con el plan                                                                               |
| `FAIL`   | La revisión pasa sin el plan, o también falla sin él pero con él imprime problemas nuevos. Bloquea el plan |
| `fails`  | La revisión falla igual sin el plan, así que no sirve para juzgar este plan. No lo bloquea                 |

Con un `FAIL`, cuilo imprime las líneas de la salida que solo aparecen con el plan, o las últimas 40 líneas de la salida. Luego se detiene y sale con el código `3`:

```text
Nothing was changed: a check fails with this plan.
```

Con `--json`, el resultado tiene `plan`, `verify` y `applied: null`.

Una revisión cuyo programa no está instalado detiene la verificación con un error como ``the check `go vet ./...` needs `go`, which is not installed``.

## Por qué pregunta primero

Las revisiones son código de tu proyecto, y se ejecutan con los archivos del plan ya en su lugar. Los archivos del pack, como una prueba o un `build.rs`, pueden ejecutarse como tú. Por eso cuilo primero imprime el plan y pregunta antes de ejecutar cualquier cosa:

```text
--verify runs the project's checks on a copy with these files in place, so code from the pack can run as you. Run the checks? [y/N]
```

Sin una terminal, o con `--no-input`, no se ejecuta nada a menos que pases `--yes`. cuilo sale con el código `2`:

```text
Nothing was run or changed: --verify runs code from the pack as you; pass --yes to run the checks.
```

`--json` nunca pregunta, así que siempre necesita `--yes`. Sin él, el resultado JSON tiene `"verify": null` y un `error` que te pide pasar `--yes`. `--dry-run` con `--verify` también necesita `--yes`: ejecuta las revisiones y no aplica nada.

Con `--yes`, cuilo ejecuta las revisiones y, si pasan, aplica el plan sin preguntar. En una terminal sin `--yes`, pregunta `Apply these changes? [y/N]` después de que pasan las revisiones.

## Confianza

`--verify` ejecuta los comandos del propio proyecto, así que solo funciona en un proyecto en el que confías. En una terminal, la primera vez, cuilo pregunta antes de armar el plan:

```text
--verify runs this project's own checks (its scripts, cargo, go, pytest…), as you. Trust this project? Your answer is remembered (`cuilo trust` changes it). [y/N]
```

Si dijiste que no, o no hay una terminal donde preguntar, la verificación se detiene:

```text
error: verify: verification runs this project's own checks, so it needs a trusted project; run `cuilo trust`
```

Consulta [Formateadores y confianza](/es-mx/guides/formatters-and-trust/).

## La copia

La copia deja fuera `.git`, `.cuilo`, la salida de compilación y las cachés: `target`, `dist`, `build`, `out`, `coverage`, `.next`, `.nuxt`, `.astro`, `.svelte-kit`, `.turbo`, `.cache`, `.output`, `.vercel` y `__pycache__`, a cualquier profundidad. Se rechaza un proyecto con más de 50,000 archivos o 1024 MB fuera de esas carpetas.

`node_modules`, `.venv` y `venv` se enlazan en la copia en lugar de copiarse, así que las revisiones usan las dependencias que ya instalaste. Como la copia comparte esas carpetas con tu proyecto, se rechaza un plan que escriba en alguna de ellas.

Las copias viven en la carpeta `verify` de tu caché de cuilo:

| Sistema      | Carpeta de caché                                   |
| ------------ | -------------------------------------------------- |
| Cualquiera   | `$CUILO_CACHE_DIR`, si está definida               |
| macOS        | `~/Library/Caches/cuilo`                           |
| Linux y Unix | `$XDG_CACHE_HOME/cuilo`, o si no, `~/.cache/cuilo` |
| Windows      | `%LOCALAPPDATA%\cuilo`                             |

En Unix, cuilo crea la carpeta para que solo tú puedas abrirla, y rechaza una que otros usuarios puedan abrir. También rechaza una carpeta dentro de otra en la que cualquier usuario puede escribir, como `/tmp`, porque las herramientas de las revisiones leen archivos de configuración de las carpetas superiores. El error termina con `which every user can write to; set CUILO_CACHE_DIR to a folder of your own`.

Cada copia se borra cuando termina la verificación. Si una ejecución interrumpida dejó copias, una ejecución posterior las borra después de 24 horas.

## Para agentes

Por MCP, `verify_plan` hace el mismo trabajo. Está marcada como una herramienta que no es de solo lectura, así que los agentes te preguntan antes de ejecutarla. Consulta [MCP](/es-mx/agents/mcp/).
