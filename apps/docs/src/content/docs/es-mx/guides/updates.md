---

title: Actualiza el código generado
description: Mueve tus piezas a una nueva versión del pack con cuilo update, que fusiona los cambios del pack con los archivos que editaste.
---

`cuilo update <pack>` mueve todas las piezas que tu proyecto tiene de un pack a otra versión de ese pack. Arma un solo plan para todas. Tú revisas el plan y, al aplicarlo, se escribe una sola transacción que [`cuilo undo`](/es-mx/reference/cli/undo/) puede revertir.

## Cómo funciona la fusión

Cuando cuilo escribe un archivo, `cuilo.lock` guarda el SHA-256 de lo que escribió como el `base` del archivo, y `.cuilo/objects/` conserva una copia de ese contenido. Al actualizar, cuilo genera la nueva versión con las entradas, la variante y el stack que registró el lockfile. Luego compara tres versiones de cada archivo: el original que escribió cuilo, el archivo como está ahora y la nueva versión.

| El archivo desde que se generó | Qué hace la actualización                                              |
| ------------------------------ | ---------------------------------------------------------------------- |
| Sin editar                     | Toma la nueva versión, o borra el archivo si la nueva versión lo quitó |
| Editado en otras líneas        | Fusiona ambos, línea por línea                                         |
| Editado en las mismas líneas   | Conflicto: consulta [Conflictos](#conflictos)                          |
| Editado, y el pack lo quitó    | Se queda como lo dejaste y deja de rastrearse                          |
| Lo borraste tú                 | Sigue borrado                                                          |

La actualización también crea los archivos nuevos de esta versión y agrega las piezas que ahora requiere. Las entradas se conservan, y las entradas nuevas toman su valor predeterminado. El texto que un parche insertó en tus propios archivos se reemplaza, a menos que lo hayas editado. Eso incluye la conexión de una pieza que agregaste con `--wire`. Una pieza que agregaste sin conexión nunca la recibe en una actualización. Cuando un parche de conexión ya no encaja, por ejemplo porque cambiaste su línea de ancla, la actualización deja ese archivo como está y anota el motivo. Una pieza que la nueva versión quitó conserva sus archivos. El plan anota las piezas quitadas y los parches que dejó sin tocar.

Si `.cuilo/` no tiene la copia de un original, por ejemplo en un clon recién hecho (la carpeta no se incluye en los commits), cuilo la reconstruye generando la versión anterior desde el origen que guarda el lockfile. Solo usa el archivo reconstruido si su hash coincide con `base`. Si ninguna de las dos opciones funciona, un archivo editado no se puede fusionar:

```text
The plan was rejected:
  src/cards/profile-card.tsx: you edited this file and its original is not available to merge with; choose markers, ours or theirs
```

## Ejecuta una actualización

Pasa el nombre del pack y, con `--from`, el origen de la nueva versión. Aquí, la carpeta de un pack local pasó de 1.0.0 a 1.1.0, y tú habías agregado un import al archivo generado:

```sh
cuilo update cards --from ./packs/cards --dry-run
```

```text
update cards 1.0.0 → 1.1.0
  modify  src/cards/profile-card.tsx
  modify  src/app.tsx
Packages to add (offered after applying; --install adds them without asking):
  npm      clsx ^2.1
Notes:
  src/cards/profile-card.tsx: the update changes this file, so its migrations did not run here
  cuilo.lock: migration to 1.1.0: `legacyTheme` is now `theme` (1 file(s) changed)
```

`--dry-run` se detiene después del plan, y `--diff` agrega el diff de cada archivo que cambia. Vuelve a ejecutar el comando sin `--dry-run` para aplicarlo: cuilo pregunta primero, y `--yes` se salta la pregunta. Tu import se queda, y la nueva prop del pack se fusiona con el archivo.

El origen debe contener el mismo pack. cuilo rechaza un origen con otro pack, y también un pack cuyos archivos cambiaron sin una nueva versión:

```text
error: pack `cards` 1.0.0 differs from the version in the lockfile; it was changed without a new version
```

Sin `--from`, cuilo usa la versión más reciente que tiene cargada. En los packs oficiales (`ui`, `backend`), es la versión que viene integrada en tu cuilo, así que al actualizar cuilo llega su siguiente versión. En otros packs, pasa `--from` con el nuevo tag, URL o carpeta. Después de la actualización, el lockfile registra ese origen. Consulta [Orígenes de packs](/es-mx/packs/sources/).

El plan lista los paquetes que agrega la nueva versión, o todos cuando no se puede cargar la versión anterior para comparar. Consulta [Paquetes que necesita una pieza](/es-mx/guides/dependencies/).

## Conflictos

Un conflicto es una región donde tú y la nueva versión cambiaron las mismas líneas, o líneas contiguas. Por defecto, la actualización rechaza todo el plan y sale con el código `3`:

```text
The plan was rejected:
  src/cards/profile-card.tsx: 1 region(s) conflict with your edits; choose markers, ours or theirs to resolve them
```

`--on-conflict` elige qué hacer en su lugar:

| Valor     | Qué pasa en cada región en conflicto                          |
| --------- | ------------------------------------------------------------- |
| `refuse`  | Se rechaza el plan (el valor predeterminado)                  |
| `markers` | Ambas versiones se escriben entre marcadores al estilo de Git |
| `ours`    | Se conservan tus líneas                                       |
| `theirs`  | Se toman las líneas de la nueva versión                       |

Los cambios fuera de las regiones en conflicto se fusionan en todos los modos. Con `markers`, el plan muestra `1 conflict(s)` junto al archivo, y el archivo recibe las tres versiones:

```sh
cuilo update cards --from ./packs/cards --on-conflict markers
```

```text
<<<<<<< yours
			<h2 className="title">{title}</h2>
||||||| original
			<h2>{title}</h2>
=======
			<h2>{title}</h2>
			{subtitle && <p>{subtitle}</p>}
>>>>>>> cards 1.1.0
```

Edita el archivo para quedarte con lo que quieras y quita los marcadores.

Cuando el original no está disponible, `ours` conserva tu archivo completo, `theirs` toma el archivo nuevo completo y `markers` pone las dos versiones completas entre marcadores. En un archivo binario que cambiaron ambos lados, solo funcionan `ours` o `theirs`.

## Revisa fragmento por fragmento

`--review` te pregunta por cada fragmento de cada cambio antes de mostrar el plan, como `git add -p`:

```text
modify src/cards/profile-card.tsx
@@ -3,6 +3,7 @@
 
 export interface ProfileCardProps {
 	title: string;
+	subtitle?: string;
 }
 
 export function ProfileCard({ title }: ProfileCardProps) {
Keep this hunk? [y]es, [n]o, [a]ll in file, [d]rop rest of file, [q]uit:
```

`a` conserva el resto de los fragmentos del archivo, `d` los omite y `q` omite todo lo que queda en el plan. Luego cuilo muestra el plan reducido y pregunta `Apply these changes? [y/N]`.

Un fragmento omitido cuenta como una edición tuya, así que las actualizaciones posteriores lo siguen omitiendo. Un archivo nuevo omitido queda fuera del lockfile, así que la siguiente actualización te lo vuelve a ofrecer. `--review` necesita una terminal. Con el valor predeterminado `refuse`, un plan con conflictos se rechaza antes de que empiece la revisión, así que primero elige otro valor de `--on-conflict`.

## Migraciones

Un pack puede incluir migraciones para código que no es suyo, como renombrar una prop en todos los lugares donde se usa. Una migración se ejecuta cuando la actualización cruza su versión, y sus ediciones aparecen en el plan como cualquier otro cambio. Los archivos que cambia la propia actualización se omiten, con una nota.

Las migraciones `rename` vienen integradas. Las migraciones `ast_grep` necesitan el programa `ast-grep`, y sin él se rechaza el plan:

```text
The plan was rejected:
  cuilo.lock: this migration needs `ast-grep`; install it, set `ast_grep` in your user config, or update without migrations
```

`--no-migrate` actualiza sin ejecutarlas. Aun así, el lockfile registra la nueva versión, así que una actualización posterior tampoco las ejecuta. Para escribir migraciones, consulta [Escribe un pack](/es-mx/packs/write-a-pack/).

## Comprueba el resultado

Después de aplicar, [`cuilo status`](/es-mx/reference/cli/status/) muestra un archivo fusionado como `modified`. Su base ahora es la nueva versión, y tus ediciones siguen ahí:

```text
modified  src/cards/profile-card.tsx
```

[`cuilo why`](/es-mx/reference/cli/why/) te dice quién escribió una línea:

```text
src/cards/profile-card.tsx:2 is yours: added or changed since generation; cards 1.1.0 piece `card`
src/cards/profile-card.tsx:6 is from the pack (line 5 of the original); cards 1.1.0 piece `card`
```

`cuilo undo` restaura la versión anterior de cada archivo y del lockfile. Para ejecutar las revisiones del proyecto antes de aplicar, agrega `--verify`. Consulta [Verifica un plan](/es-mx/guides/verify/). Para ver todas las opciones, consulta la [referencia de `cuilo update`](/es-mx/reference/cli/update/).
