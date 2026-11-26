---

title: Paquetes que necesita una pieza
description: Cómo los planes listan los paquetes que importa una pieza, y cómo cuilo ofrece agregarlos con el gestor de paquetes de tu proyecto.
---

Una pieza puede nombrar los paquetes que importa su código, como `clsx` para un componente o `zod` para un recurso. cuilo los lista en el plan y, después de aplicar, te ofrece el comando que agrega los que faltan con el gestor de paquetes de tu proyecto.

## En el plan

Los paquetes aparecen bajo su propio encabezado, con su ecosistema y su rango de versiones. Los paquetes de desarrollo se marcan con `(dev)`:

```text
add pyx/client
  create  app/client.py
Packages to add (offered after applying; --install adds them without asking):
  pypi     httpx >=0.28
  pypi     pytest >=8 (dev)
```

Una actualización lista los paquetes que agrega la nueva versión. Consulta [Actualiza el código generado](/es-mx/guides/updates/).

Para declarar paquetes, un pack debe definir `permissions.dependencies = true` en `pack.toml`. Si no, cuilo se niega a cargarlo:

```text
error: `pieces/card/piece.toml`: declares dependencies, but `pack.toml` does not set `permissions.dependencies`
```

Para declarar paquetes en tu propio pack, consulta [Escribe un pack](/es-mx/packs/write-a-pack/) y el [manifiesto de pieza](/es-mx/reference/piece-manifest/).

## Después de aplicar

cuilo aplica primero el plan. Luego arma el comando para los paquetes que tu proyecto todavía no declara, y pregunta:

```text
Apply these changes? [y/N] y
Applied transaction 1 (1 file(s)). Undo it with `cuilo undo`.
The plan lists packages to add:
  bun add clsx@^2.1
Run this? [y/N]
```

Si respondes que no, cuilo imprime el comando para que lo ejecutes tú:

```text
Add the packages with:
  bun add clsx@^2.1
```

Los paquetes que el proyecto ya declara se omiten, así que sus versiones nunca cambian. Aquí `httpx` ya está en `pyproject.toml`, así que solo queda `pytest`:

```text
Add the packages with:
  uv add --dev 'pytest>=8'
```

Cuando todos los paquetes ya están declarados, cuilo no pregunta nada.

## Elige sin que cuilo pregunte

| Opción         | Qué pasa                                             |
| -------------- | ---------------------------------------------------- |
| `--install`    | Ejecuta el comando después de aplicar, sin preguntar |
| `--no-install` | Nunca lo ofrece                                      |
| ninguna        | Pregunta en una terminal; si no, imprime el comando  |

Con `--install`, cuilo imprime cada comando antes de ejecutarlo desde la raíz del proyecto:

```text
Applied transaction 2 (1 file(s)). Undo it with `cuilo undo`.
$ bun add clsx@^2.1
```

Si el comando falla, los archivos generados se quedan aplicados, y cuilo sale con el código `1`:

```text
error: `bun add clsx@^2.1` failed (exit status: 1); the generated files are applied, so add the packages yourself
```

Con `--json`, el resultado tiene un objeto `install` con los comandos (`commands`), los paquetes que ya están (`present`), los que hay que agregar a mano (`manual`) y si los comandos se ejecutaron (`ran`).

## Gestores de paquetes

cuilo usa el gestor de paquetes que tu proyecto ya usa:

| Ecosistema  | Gestor de paquetes          | Comando                                                            |
| ----------- | --------------------------- | ------------------------------------------------------------------ |
| `npm`       | bun, pnpm, yarn, deno o npm | `bun add`, `pnpm add`, `yarn add`, `deno add npm:…`, `npm install` |
| `cargo`     | cargo                       | `cargo add`, un paquete a la vez                                   |
| `pypi`      | uv, Poetry, PDM o Pipenv    | `uv add`, `poetry add`, `pdm add`, `pipenv install`                |
| `go`        | go                          | `go get`                                                           |
| `pub`       | Flutter o Dart              | `flutter pub add`, o si no, `dart pub add`                         |
| `rubygems`  | Bundler                     | `bundle add`                                                       |
| `packagist` | Composer                    | `composer require`                                                 |

Los paquetes de desarrollo usan la opción propia de cada herramienta, como `-d` en bun, `--dev` en cargo y uv, o `--group dev` en Poetry.

En JavaScript, decide el campo `packageManager` de `package.json`. Si no existe, decide el lockfile: `bun.lock`, `pnpm-lock.yaml`, `yarn.lock` o `package-lock.json`. Si no hay ninguno, cuilo usa deno en un proyecto de Deno y npm en cualquier otro. En Python, decide `uv.lock`, la configuración de Poetry o `poetry.lock`, `pdm.lock` o `Pipfile`. [`cuilo detect`](/es-mx/reference/cli/detect/) muestra lo que encontró cuilo.

Cuando no hay un gestor de paquetes que cuilo pueda ejecutar, como en un proyecto de Python que solo tiene `requirements.txt`, te dice qué agregar a mano:

```text
Add by hand: add `httpx>=0.28` to requirements.txt or pyproject.toml
Add by hand: add `pytest>=8` to requirements.txt or pyproject.toml
```

## Deshacer e instalaciones

Los cambios del gestor de paquetes quedan fuera del historial de cuilo. `cuilo undo` revierte los archivos generados y el lockfile, pero no los paquetes que se agregaron. Quítalos con tu gestor de paquetes. Consulta [Historial y deshacer](/es-mx/guides/history/).
