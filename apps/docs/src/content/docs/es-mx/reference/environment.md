---

title: Variables de entorno
description: Todas las variables de entorno que leen cuilo y sus scripts de instalación, y lo que cambia cada una.
---

cuilo lee pocas variables de entorno. Todo lo que manda credenciales o corre programas se define en tu [configuración de usuario](/es-mx/reference/user-config/), que nombra variables en lugar de guardar secretos.

## cuilo

| Variable                         | Qué hace                                                                                                                                                                                                                                                                             |
| -------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `CUILO_CONFIG_DIR`               | La carpeta de tu configuración de usuario, `config.toml`, y de `trusted-projects.toml`, los proyectos para los que respondiste la pregunta de confianza.                                                                                                                             |
| `CUILO_CACHE_DIR`                | La carpeta de los packs descargados (`packs/`), de las copias que revisa `--verify` (`verify/`) y de lo que encontró la última búsqueda de actualizaciones (`update.json`).                                                                                                          |
| `CUILO_UPDATES`                  | `auto`, `notify` u `off`, en lugar de `updates` en tu configuración de usuario: qué hace cuilo con las versiones nuevas. Consulta [actualizaciones automáticas](/es-mx/install/#actualizaciones-automáticas). Vacía cuenta como no definida; cualquier otro valor cuenta como `off`. |
| `CI`                             | Cuando tiene un valor que no está vacío, cuilo nunca busca versiones nuevas ni imprime avisos de actualización.                                                                                                                                                                      |
| `NO_COLOR`                       | Desactiva los colores. Defínela con cualquier valor, incluso uno vacío. Los colores también se desactivan cuando la salida no es una terminal, y con `--json`.                                                                                                                       |
| La variable de `auth.<host>.env` | El token que manda cuilo cuando descarga un pack desde ese host, por ejemplo `GITHUB_TOKEN`. Si no está definida o está vacía, no se manda ningún token; un token con espacios o caracteres de control es un error.                                                                  |
| `GIT_SSH_COMMAND`, `GIT_SSH`     | Cuando alguna está definida, o git tiene `core.sshCommand`, cuilo le deja SSH a ese comando para los orígenes `git:`. Si no, corre `ssh -o BatchMode=yes -o ConnectTimeout=30`.                                                                                                      |
| `PATH`                           | Donde cuilo encuentra `git`, `ast-grep`, los gestores de paquetes, las revisiones que corre `--verify` y los formateadores de tu sistema, como rustfmt, gofmt, y Ruff o Black sin un `.venv`. En Windows, cuilo también lee `PATHEXT` para encontrar shims como `npm.cmd`.           |

Un `CUILO_CONFIG_DIR` o `CUILO_CACHE_DIR` vacío cuenta como no definido. Sin ellos, cuilo usa las carpetas del sistema:

| Carpeta       | macOS                                              | Linux                                              | Windows                |
| ------------- | -------------------------------------------------- | -------------------------------------------------- | ---------------------- |
| Configuración | `$XDG_CONFIG_HOME/cuilo`; si no, `~/.config/cuilo` | `$XDG_CONFIG_HOME/cuilo`; si no, `~/.config/cuilo` | `%APPDATA%\cuilo`      |
| Caché         | `~/Library/Caches/cuilo`                           | `$XDG_CACHE_HOME/cuilo`; si no, `~/.cache/cuilo`   | `%LOCALAPPDATA%\cuilo` |

`~` es `$HOME`. Por eso cuilo también lee `HOME`, `XDG_CONFIG_HOME`, `XDG_CACHE_HOME`, `APPDATA` y `LOCALAPPDATA`.

:::caution
`--verify` necesita una carpeta de caché que solo tú puedas abrir, sin ninguna carpeta superior en la que cualquier usuario pueda escribir. Una caché en `/tmp` se rechaza. Si la tuya es compartida, define `CUILO_CACHE_DIR` con una carpeta propia y corre `chmod 700` sobre ella.
:::

### Proxies y certificados

El cliente HTTPS y el cargador de certificados de cuilo leen estas variables al descargar packs, en `cuilo upgrade` y al buscar actualizaciones:

| Variable                                                             | Qué hace                                                                                                                  |
| -------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| `ALL_PROXY`, `HTTPS_PROXY`, `HTTP_PROXY`, o las mismas en minúsculas | Un proxy HTTP para las descargas. Se usa la primera válida, en ese orden.                                                 |
| `NO_PROXY`, `no_proxy`                                               | Hosts que no pasan por el proxy.                                                                                          |
| `SSL_CERT_FILE`, `SSL_CERT_DIR`                                      | Certificados PEM en los que confiar. Cuando alguna está definida, cuilo confía solo en esos, en lugar de los del sistema. |

### Variables para los programas que corre cuilo

- Las revisiones que corre `--verify` reciben `CI=1` y un `CARGO_TARGET_DIR` dentro de la caché de cuilo.
- `git` corre para los orígenes `git:` con `GIT_TERMINAL_PROMPT=0`, un `GIT_ASKPASS` vacío y `SSH_ASKPASS_REQUIRE=never`, así que nunca pregunta nada.

## Scripts de instalación

`install.sh` (macOS y Linux) e `install.ps1` (Windows) leen:

| Variable                    | Qué hace                                                                                                                                                                                                                                                                  |
| --------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `CUILO_VERSION`             | La versión que se instala, por ejemplo `2.0.0`; se quita la `v` inicial si la tiene. Un argumento de `install.sh` o `-Version` de `install.ps1` toma su lugar. Por defecto: la versión estable más nueva, o la versión preliminar más nueva mientras no haya una estable. |
| `CUILO_INSTALL`             | La carpeta de instalación. cuilo queda en su carpeta `bin`. Por defecto: `~/.cuilo`, o `~\.cuilo` en Windows.                                                                                                                                                             |
| `CUILO_REQUIRE_SIGNATURE=1` | No instala a menos que `minisign` esté en el `PATH` para revisar la firma. Sin esta variable, los scripts revisan el SHA-256, y también la firma cuando `minisign` está disponible.                                                                                       |
| `CUILO_NO_MODIFY_PATH=1`    | Solo `install.ps1`: no toca el `Path` de tu usuario. Por defecto, agrega la carpeta `bin`.                                                                                                                                                                                |

`install.sh` también lee `HOME` para la carpeta por defecto y `PATH` para decirte si tienes que agregarle la carpeta `bin`. `install.ps1` lee `PROCESSOR_ARCHITEW6432` o `PROCESSOR_ARCHITECTURE` para elegir la compilación. Consulta [instalación](/es-mx/install/).
