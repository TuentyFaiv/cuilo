---

title: Orígenes de packs
description: Desde dónde puede cargar cuilo un pack, qué fija cuilo.lock, los espacios de nombres en cuilo.json y las credenciales de orígenes privados.
---

Un origen es un texto que indica de dónde viene un pack. Pásalo a `--from` en `cuilo add`, `cuilo adopt` y `cuilo update`, o revísalo antes con `cuilo pack add <source>`, que carga y verifica el pack sin cambiar el proyecto.

## Formas de un origen

| Origen                                | Ejemplo                                                                             |
| ------------------------------------- | ----------------------------------------------------------------------------------- |
| Oficial, incluido en cuilo            | `ui`, `official:backend`                                                            |
| Carpeta                               | `./packs/mine`, `../shared/ui`, `path:/opt/packs/mine`                              |
| Tag o commit de GitHub                | `gh:acme/packs@v1.2.0`, `gh:acme/packs/ui@v1.2.0`                                   |
| Tarball por HTTPS                     | `https://example.com/ui-1.2.0.tar.gz`                                               |
| Cualquier repositorio de git, por SSH | `git:git@github.com:acme/packs.git@v1.2.0`, `git:ssh://git@host/x.git//packs/ui@v2` |
| Elemento de registro                  | `registry:https://acme.dev/r/card.json`, `registry:@acme/card`                      |
| Espacio de nombres del proyecto       | `@acme/ui`                                                                          |

### Oficial

`ui` y `backend` vienen dentro del binario de cuilo, así que cargan sin descargar nada, y una versión más nueva de cuilo trae su siguiente versión. Un nombre sin prefijo se lee como un pack oficial; `official:<name>` lo indica de forma explícita. Consulta [ui](/es-mx/directory/ui/) y [backend](/es-mx/directory/backend/).

### Carpeta

Un origen que empieza con `./`, `../`, `/` o `.\` es una carpeta, igual que cualquier cosa después de `path:`. Las carpetas relativas se leen desde la raíz del proyecto. Así [pruebas un pack](/es-mx/packs/write-a-pack/#pruébalo-localmente) mientras lo escribes.

### GitHub

`gh:owner/repo@ref` descarga el repositorio en un tag o commit. Agrega una carpeta después del repositorio cuando el pack no está en su raíz: `gh:acme/packs/ui@v1.2.0`. El `@ref` es obligatorio.

Sin token, cuilo descarga desde `codeload.github.com`. Con un token para `github.com` en tu configuración de usuario, descarga a través de la API de GitHub, que también funciona con repositorios privados.

### Tarball por HTTPS

Una URL `https://` cuya ruta termina en `.tar.gz` o `.tgz`. Si el tarball mete todos los archivos en una sola carpeta superior, como suelen hacerlo los de las versiones publicadas, cuilo quita esa carpeta.

### Git por SSH

`git:` acepta una URL `ssh://` o una URL `user@host:path`, seguida de `@ref` con un tag o commit. Agrega `//folder` antes de la `@` para elegir una carpeta dentro del repositorio. Así llegas a repositorios de GitLab, de Gitea y autohospedados.

cuilo corre tu propio `git`, así que se usan tus llaves SSH, tu agente y tu `~/.ssh/config`. Solo permite el protocolo SSH, trae un solo commit y lo lee con `git archive`, así que no hace checkout de nada. Nunca te pregunta nada: a menos que definas tu propio comando de SSH con `GIT_SSH_COMMAND`, `GIT_SSH` o `core.sshCommand` de git, SSH corre con `BatchMode=yes` y un tiempo de espera de conexión de 30 segundos. Cada comando de git se detiene después de dos minutos. Las URLs `git:` por HTTPS se rechazan; para esas, usa `gh:` o un tarball.

### Elemento de registro

`registry:` carga un elemento de registro (registry item) en el formato `registry-item.json` y lo convierte en un pack llamado `registry-<item>`. Consulta [elementos de registro](/es-mx/packs/registry-items/).

### Espacio de nombres

`@namespace/pack` nombra un pack dentro de un origen que defines una sola vez en la configuración del proyecto, como un espacio de nombres (namespace):

```json
{
	"sources": {
		"@acme": "gh:acme/packs@v1.2.0"
	}
}
```

Después usa `@acme/ui` en cualquier lugar donde va un origen, por ejemplo `cuilo add button --from @acme/ui`.

Un espacio de nombres apunta a un origen de carpeta, `gh:`, `git:` o `https://`. Ahí, cuilo busca `packs/<pack>/pack.toml`; si no existe, usa el pack de la raíz del origen. El pack conserva el nombre de su propio `pack.toml`, y ese nombre no puede ser `ui` ni `backend`. Un espacio de nombres desconocido falla con ``namespace `@acme` is not configured; add it to `sources` in the project config``. Consulta [`sources`](/es-mx/reference/configuration/#sources).

El lockfile registra el origen como `@acme/ui`. Cuando apuntas el espacio de nombres a un tag nuevo, el pack fijado ya no coincide, así que pasa a la versión nueva con `cuilo update <pack> --from @acme/ui`.

## Qué fija cuilo.lock

`cuilo pack add` solo carga y revisa. Cuando aplicas un plan de un pack, `cuilo.lock` lo registra:

```toml
[[pack]]
name = "acme-ui"
version = "0.1.0"
source = "path:./my-pack"
digest = "sha256:7ed4cf6fb1364c68d53fc6d1a941d8e5ed266bf41cda4801d5d4bafd80ea8c69"
```

- `source` es el origen en su forma completa: `./my-pack` se vuelve `path:./my-pack` y `ui` se vuelve `official:ui`. Una carpeta dada como ruta absoluta no recibe `source`, porque solo existe en tu máquina, así que vuelve a pasar `--from` para usarla.
- `digest` es el SHA-256 de todos los archivos del pack excepto su firma.
- `signer` se agrega cuando una llave de confianza firmó el pack; consulta [firma un pack](/es-mx/packs/signing/).

Cada comando carga los packs que fija el lockfile, así que los comandos que siguen, como `cuilo add acme-ui/card`, los encuentran sin `--from`. Los packs descargados se guardan por digest en la carpeta `packs` de tu caché de cuilo, así que un pack fijado carga sin conexión. Si un origen ahora entrega otros archivos para lo que ya quedó fijado, cuilo lo rechaza:

```text
error: `path:./my-pack` changed: the lockfile pinned sha256:7ed4…, the source now has sha256:45ad…
```

Para pasar a archivos nuevos, sube la versión del pack y corre `cuilo update <pack> --from <source>`. Un registro del lockfile solo carga el pack y la versión que nombra. Los packs oficiales no se comparan con un digest fijado: vienen con cuilo, así que una versión más nueva de cuilo trae su siguiente versión. Consulta la [referencia del lockfile](/es-mx/reference/lockfile/).

## Orígenes privados

Las credenciales viven en tu [configuración de usuario](/es-mx/reference/user-config/#auth), `config.toml` en tu carpeta de configuración de cuilo, nunca en la configuración del proyecto. Un proyecto clonado no puede mandar tus tokens a ningún lado. La configuración solo guarda el nombre de una variable de entorno:

```toml
[auth."github.com"]
env = "GITHUB_TOKEN"
```

cuilo manda el token como `Authorization: Bearer <token>`. `header` cambia el encabezado y `scheme` la palabra que va antes del token; un `scheme` vacío manda solo el token:

```toml
[auth."gitlab.example.com"]
env = "GITLAB_TOKEN"
header = "PRIVATE-TOKEN"
scheme = ""
```

La clave es el host de la URL de descarga; usa `github.com` para los orígenes `gh:`. Los tokens solo van a ese host, nunca a un host al que te redirija una descarga. Cuando la variable no está definida o está vacía, cuilo no manda el encabezado. Cuando una descarga sin token recibe HTTP 401, 403 o 404, el error sugiere agregar una tabla `[auth."<host>"]`, o avisa que la variable configurada no está definida. Los orígenes `git:` no usan esta tabla; usan tu configuración de SSH.

## Nombres reservados

Los nombres `ui` y `backend` son de los packs oficiales y solo se cargan desde el propio cuilo. Un pack con uno de esos nombres que venga de cualquier otro origen se rechaza:

```text
error: `path:./bad` is a pack named `ui`, the name of a pack built into cuilo; packs from other sources need another name
```

## Descargas

Las descargas solo usan HTTPS, verificado contra los certificados en los que confía tu sistema operativo. cuilo sigue como máximo 5 redirecciones, todas a HTTPS, se rinde después de 60 segundos y se detiene en 64 MiB. Las variables de proxy y de certificados están en [variables de entorno](/es-mx/reference/environment/).

En una compilación de cuilo sin la feature `remote` de `cuilo-packs`, que viene activada por defecto, los orígenes remotos solo cargan desde la caché y los orígenes `git:` se rechazan.
