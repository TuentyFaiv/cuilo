---

title: Códigos de salida
description: Los cuatro códigos de salida que devuelve cuilo, cuándo pasa cada uno y qué imprime, para scripts, CI y agentes de IA.
---

Cada comando termina con uno de cuatro códigos de salida, así que los scripts y los agentes pueden distinguir un comando que terminó de una pregunta que cuilo no pudo hacer.

| Código | Significado                     |
| ------ | ------------------------------- |
| `0`    | Listo                           |
| `1`    | Falló                           |
| `2`    | Uso, o se necesita confirmación |
| `3`    | Plan rechazado                  |

## 0: listo

El comando hizo lo que pediste, o no había nada que hacer. Esto incluye:

- Un plan que se muestra con `--dry-run`, y un plan sin nada que cambiar.
- Un plan al que respondiste que no en `Apply these changes?`, en una terminal. cuilo imprime `Nothing was changed.`
- Revisiones a las que respondiste que no en la pregunta de `--verify`, en una terminal. cuilo imprime `Nothing was run or changed.`
- `--help`, `--version` y `cuilo` sin comando, que imprime la ayuda.

## 1: falló

Algo salió mal, y cuilo imprime `error: <message>` en stderr. Con `--json`, en cambio, imprime `{ "error": "<message>" }` en stdout. Causas comunes:

- Entradas que no encajan con la pieza, como ``input `name` is required`` o ``input `size` must be one of: small, large``.
- Un pack o una pieza que no está cargado, como ``no pack `acme-ui` is loaded; use --from <source> to load it``.
- Un origen que no se puede cargar ni verificar: un origen mal escrito, una descarga fallida, un pack que no pasa la validación, una firma rechazada, un origen cuyos archivos cambiaron desde que `cuilo.lock` los fijó, o un pack que usa un nombre oficial.
- Un parche que no se puede aplicar, como un archivo que falta o un ancla que falta o no es única.
- Una configuración de usuario o de proyecto que no se puede leer.
- `--review` sin terminal.
- `cuilo undo` cuando los archivos cambiaron después de esa transacción, a menos que pases `--force`.
- Un comando del gestor de paquetes que falla después de aplicar el plan con `--install`. Los archivos generados se quedan aplicados; agrega los paquetes tú.

## 2: uso, o se necesita confirmación

La línea de comandos no es válida, o el comando necesita una respuesta que no puede obtener:

- Un flag desconocido, un argumento que falta o un valor inválido. El analizador de argumentos imprime el problema y una línea de uso.
- Un plan que necesita tu aprobación sin una terminal, o con `--json`. cuilo imprime `Nothing was changed; pass --yes to apply without asking, or --dry-run to only plan.`, o con `--json` agrega `"error": "pass --yes to apply"`. No se escribe nada.
- `--verify` sin `--yes` y sin terminal, o con `--json`. cuilo imprime `Nothing was run or changed: --verify runs code from the pack as you; pass --yes to run the checks.` No se corre nada.

Vuelve a correr el comando con `--yes`, después de leer el plan con `--dry-run` si lo necesitas.

## 3: plan rechazado

cuilo armó el plan del cambio y se negó a aplicarlo. No se escribe nada. En modo texto, lista cada problema en stderr:

```text
The plan was rejected:
  lib/cards/team-card/team-card.tsx: the pack does not declare permission to write here
```

Con `--json`, imprime los problemas en stdout:

```json
{
	"error": "plan rejected",
	"problems": [
		{
			"kind": "not_declared",
			"message": "the pack does not declare permission to write here",
			"path": "lib/cards/team-card/team-card.tsx"
		}
	]
}
```

Cada problema tiene uno de estos tipos:

| `kind`         | Por qué                                                                                                                                         |
| -------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| `conflict`     | Ya existe un archivo con otro contenido (`--force` lo reemplaza), cambió mientras se armaba el plan, o una actualización choca con tus cambios. |
| `not_declared` | El `permissions.write` del pack no cubre la ruta.                                                                                               |
| `not_granted`  | No le diste permiso al pack para escribir ahí, o para agregar dependencias.                                                                     |
| `forbidden`    | Ningún pack puede escribir ahí, como en `.git/`, `.cuilo/` o `cuilo.lock`.                                                                      |
| `unsafe`       | La ruta pasa por un enlace simbólico o por un archivo.                                                                                          |
| `unavailable`  | Falta una herramienta que necesita el plan, como `ast-grep` para una migración.                                                                 |

`--verify` también sale con `3` cuando una revisión falla por culpa del plan. cuilo imprime `Nothing was changed: a check fails with this plan.`, o con `--json`, el plan y los resultados de las revisiones con `"applied": null`. Consulta [verifica un plan](/es-mx/guides/verify/).

Para los conflictos de actualización, elige otro valor de `--on-conflict`: `markers`, `ours` o `theirs`. Consulta [actualización de packs](/es-mx/guides/updates/).
