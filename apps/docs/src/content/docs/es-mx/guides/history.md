---

title: Historial y deshacer
description: Cada plan aplicado es una transacción en .cuilo/history; cuilo log las lista y cuilo undo revierte la última sin perder tus ediciones posteriores.
---

Cada plan que cuilo aplica se convierte en una **transacción**. cuilo la registra en `.cuilo/history` antes de escribir cualquier cosa, para que [`cuilo undo`](/es-mx/reference/cli/undo/) pueda dejar cada archivo y el lockfile como estaban.

## Qué registra una transacción

Cada transacción es un archivo TOML que lleva su id como nombre, como `.cuilo/history/00000002.toml`. Guarda un resumen, si terminó, el SHA-256 de cada archivo antes y después, y el del lockfile antes y después:

```toml
id = 2
time = 1795661478
summary = "update cards 1.0.0 → 1.1.0"
state = "applied"
lock_before = "sha256:019b8b0575c1bbe1aa9715239b25772f86809bbb06b5f4963423f50b82fbfa20"
lock_after = "sha256:3cab1264f32acd1a05cd601d4138a4719e33e38d0bdccfbbc20f58041d544dc2"

[[change]]
path = "src/cards/profile-card.tsx"
before = "sha256:102a79a37f7bf3eb1a4652027657f86321cafc16a94d5fdab0461e84a8a68518"
after = "sha256:d047e9988205f56ab0bbd1a1b27b4f5340d6dc8fb84963a0cf7c4c9381878279"

[[change]]
path = "src/app.tsx"
before = "sha256:02406feece435d3a980369d5c9da74b96f84fe3af9a628efd7aa16b82de7dd95"
after = "sha256:2264dd961c2677f19322a5195974cadc59297f8e1a063702354592b2473eef8f"
```

Si falta `before`, el archivo no existía, y si falta `after`, la transacción lo borró. Una transacción que creó carpetas las lista en `created_dirs`. El contenido detrás de cada hash se guarda en `.cuilo/objects/`, que también conserva los originales con los que se fusionan las [actualizaciones](/es-mx/guides/updates/).

`.cuilo/` tiene su propio `.gitignore`, así que Git la ignora. El historial es local a tu copia del proyecto. Puedes borrar la carpeta sin problema, pero pierdes la opción de deshacer, y las actualizaciones tienen que reconstruir los originales desde la versión anterior del pack.

## Cómo se aplica un plan

Solo un proceso de cuilo cambia un proyecto a la vez. Mientras trabaja, mantiene `.cuilo/busy`. Antes de escribir, cuilo comprueba que cada archivo siga teniendo el contenido que vio el plan. Si cambiaste alguno mientras tanto, se detiene:

```text
error: `src/cards/profile-card.tsx` changed after the plan was made; plan again
```

Luego guarda la transacción como `pending`, escribe los archivos y el lockfile, y marca la transacción como `applied`. Si falla una escritura, cuilo restaura lo que ya había escrito y descarta la entrada. Si el proceso se interrumpe a la mitad, la entrada se queda como `pending`, y la siguiente aplicación se niega a empezar:

```text
error: transaction 2 was interrupted; run undo to roll it back first
```

En ese caso, `cuilo undo` revierte la transacción interrumpida.

## Lista las transacciones

[`cuilo log`](/es-mx/reference/cli/log/) lista las transacciones que puedes deshacer, de la más reciente a la más antigua, con cuántos archivos cambió cada una:

```sh
cuilo log
```

```text
   2  update cards 1.0.0 → 1.1.0  (2 file(s))
   1  add cards/card  (1 file(s))
```

`cuilo adopt` y algunas actualizaciones no cambian ningún archivo y solo escriben el lockfile, así que muestran `(0 file(s))`. Si no hay nada que deshacer, `cuilo log` imprime `Nothing to undo.` Usa `--json` para ver las entradas completas.

## Deshaz la última transacción

`cuilo undo` revierte la transacción más reciente:

```sh
cuilo undo
```

```text
Undid transaction 2: update cards 1.0.0 → 1.1.0
```

Restaura el contenido anterior de cada archivo, borra los archivos que creó la transacción, restaura el lockfile y quita las carpetas que creó la transacción si ahora están vacías. Ejecútalo otra vez para deshacer la transacción anterior. La entrada deshecha se mueve a `.cuilo/history/undone/`. No hay forma de rehacer, y las transacciones nuevas siguen contando desde el id más alto.

## Lo que undo rechaza

### Archivos que cambiaste después

`cuilo undo` comprueba que cada archivo, y `cuilo.lock`, siga teniendo lo que dejó ahí la transacción. Si editaste un archivo desde entonces, deshacer tiraría tu edición, así que se niega y lista los archivos:

```text
error: these files changed after transaction 1; undo with force to discard the edits: src/cards/profile-card.tsx
```

`cuilo undo --force` la deshace de todos modos, y pierdes tus ediciones en esos archivos. Si quieres conservarlas, haz commit o cópialas antes.

### Rutas que cuilo nunca escribe

El registro vive en la carpeta del proyecto, así que un proyecto que recibes de otra persona, como un archivo comprimido, podría traer una entrada que alguien escribió a mano. `cuilo undo` rechaza cualquier entrada que mencione una ruta en la que ningún pack puede escribir, como `.git/`, incluso con `--force`:

```text
error: history entry 2 names `.git/hooks/pre-commit`, which cuilo never writes; delete the entry from `.cuilo/history` if you did not create it
```

### Contenido faltante u otro proceso

`cuilo undo` se detiene si ya no está en `.cuilo/objects/` el contenido que necesita, y cuando otro proceso de cuilo tiene tomado el proyecto:

```text
error: another cuilo process is changing this project (remove `.cuilo/busy` if it crashed)
```

## Lo que el historial no cubre

Deshacer solo revierte lo que escribió cuilo. Los paquetes que un gestor de paquetes agregó después de que aplicaste un plan quedan fuera del historial, así que `cuilo undo` no los toca. Consulta [Paquetes que necesita una pieza](/es-mx/guides/dependencies/). Las ediciones que haces a mano tampoco son transacciones.
