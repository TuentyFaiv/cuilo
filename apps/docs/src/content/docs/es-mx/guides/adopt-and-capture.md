---

title: Adopta y captura
description: Toma el control del código que ya tienes con cuilo adopt, o convierte archivos del proyecto en una pieza reutilizable de un pack local con cuilo pack capture.
---

Dos comandos conectan cuilo con código que ya existe. `cuilo adopt` registra archivos existentes como si una pieza los hubiera generado, para que las actualizaciones posteriores se fusionen con ellos. `cuilo pack capture` va en sentido contrario: convierte archivos de tu proyecto en una pieza nueva que puedes volver a generar con otros nombres.

## Adopta código existente

Usa [`cuilo adopt`](/es-mx/reference/cli/adopt/) cuando tu proyecto tiene código que corresponde a una pieza: lo escribiste a mano, lo copiaste del pack o lo hiciste con otra herramienta. Recibe los mismos argumentos que `cuilo add`:

```sh
cuilo adopt cards/card ProfileCard --from ./packs/cards --dry-run
```

```text
adopt cards/card
  nothing to change
Packages to add (offered after applying; --install adds them without asking):
  npm      clsx ^2.1
Notes:
  src/cards/profile-card.tsx: adopted with your differences; updates will merge into them
```

cuilo genera la pieza con tus entradas y revisa cada archivo que escribiría:

- Un archivo que ya existe se queda como está. El lockfile registra como su original lo que el pack habría escrito, y `.cuilo/` guarda una copia.
- Un archivo que no existe se crea, igual que con `cuilo add`.

Los parches que la pieza hace en otros archivos se planean igual que con `cuilo add`, y un parche que ya está aplicado no cambia nada. El plan de arriba no cambia ningún archivo, así que al aplicarlo solo se escribe el lockfile, como una transacción:

```text
Applied transaction 1 (0 file(s)). Undo it with `cuilo undo`.
```

Tus diferencias con el pack ahora cuentan como ediciones tuyas:

```sh
cuilo status
cuilo why src/cards/profile-card.tsx:9
```

```text
modified  src/cards/profile-card.tsx
src/cards/profile-card.tsx:9 is yours: added or changed since generation; cards 1.0.0 piece `card`
```

Cuando el pack publica una nueva versión, `cuilo update` la fusiona con tu archivo como con cualquier archivo generado. Aquí el pack agregó una prop `subtitle`, y la clase extra `"shadow"` que tenías se queda:

```sh
cuilo update cards --from ./packs/cards
```

```tsx
export function ProfileCard({ title, subtitle }: ProfileCardProps) {
	return (
		<article className={clsx("profile-card", "shadow")}>
			<h2>{title}</h2>
			{subtitle && <p>{subtitle}</p>}
		</article>
	);
}
```

Consulta [Actualiza el código generado](/es-mx/guides/updates/).

:::tip
Usa `--dry-run` primero. Si el plan muestra `create` para archivos que esperabas adoptar, el nombre o las entradas no coinciden con las rutas que usa tu código, y cuilo agregaría archivos nuevos en lugar de adoptar los tuyos.
:::

## Captura archivos como una pieza

[`cuilo pack capture`](/es-mx/reference/cli/pack/#cuilo-pack-capture) copia archivos del proyecto a una pieza nueva de un pack local. `--piece` le da nombre a la pieza, y `--name` indica cómo está escrito el nombre del componente en los archivos. Esa forma de escribirlo se convierte en la entrada `name` de la pieza, así que cuilo la reemplaza en todas sus formas (`UserCard`, `user-card`…) cuando vuelves a generar la pieza:

```sh
cuilo pack capture src/ui/user-card/user-card.tsx src/ui/user-card/user-card.module.css --piece card --name UserCard
```

```text
capture card into packs/local
  create  packs/local/pieces/card/files/user-card/user-card.tsx
  create  packs/local/pieces/card/files/user-card/user-card.module.css
  create  packs/local/pieces/card/piece.toml
  create  packs/local/pack.toml
```

Sin `--name`, la pieza no tiene la entrada `name` y genera los archivos tal como están.

El plan solo escribe dentro de la carpeta del pack, `packs/local` por defecto. `--pack-dir` elige otra carpeta, y `--pack` le da nombre a un pack nuevo (`local` por defecto). Un `pack.toml` nuevo empieza en la versión `0.1.0` y puede escribir en la carpeta que comparten los archivos:

```toml
schema_version = 1
name = "local"
version = "0.1.0"

[permissions]
write = ["src/ui/**"]
```

El `target` de la pieza es esa carpeta compartida. Las carpetas que llevan el nombre de `--name`, como `user-card/`, se quedan dentro de la pieza, así que también se renombran.

El nombre necesita dos o más palabras, porque un nombre de una sola palabra solo se reemplaza donde aparece escrito exactamente igual:

```text
error: cannot capture these files: capture: `Card` is one word, so it would only be replaced where written exactly `Card`; file names and other spellings would keep it. Use a name of two or more words, such as `SiteCard`
```

cuilo también rechaza archivos que mencionan `@cuilo`, que las plantillas de los packs reservan para directivas. Una pieza que ya existe solo se reemplaza con `--force`.

### Genera la pieza capturada

Carga el pack con `--from` y agrega la pieza con un nombre nuevo:

```sh
cuilo add card TeamCard --from ./packs/local
```

```text
add local/card
  create  src/ui/team-card/team-card.module.css
  create  src/ui/team-card/team-card.tsx
```

Los archivos originales todavía no se rastrean. Para que las versiones posteriores del pack también se fusionen con ellos, adóptalos:

```sh
cuilo adopt card UserCard --from ./packs/local
```

### Captura en un pack existente

Cuando el pack ya tiene un `pack.toml`, cuilo no lo toca. Si su `permissions.write` no cubre los archivos nuevos, el plan te dice qué agregar:

```text
Notes:
  packs/local/pack.toml: add `lib/hooks/**` to `permissions.write` so the pack may write this piece's files
```

Después de cambiar un pack que el proyecto ya usa, sube su `version` en `pack.toml`, porque cuilo rechaza un pack cuyos archivos cambiaron sin cambiar de versión. Luego mueve el proyecto a esa versión:

```sh
cuilo update local --from ./packs/local
```

Mientras no lo hagas, `cuilo add --from ./packs/local` falla, porque el lockfile fijó los archivos anteriores:

```text
error: `path:./packs/local` changed: the lockfile pinned sha256:5e492cd3dac90e8d7795250d25c44a9f717c11720912d2ba0d46402ec9057b29, the source now has sha256:8b28b334e39e41a5186998523bf7507d85ae770adefb566b5160f9e802c7f98f
```

Una pieza capturada es una copia simple de tus archivos. Para agregar código opcional, variantes o más entradas, edítala como se explica en [Escribe un pack](/es-mx/packs/write-a-pack/).
