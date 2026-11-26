---

title: Environment variables
description: Every environment variable cuilo and its install scripts read, and what each one changes.
---

cuilo reads few environment variables. Anything that sends credentials or runs programs is set in your [user config](/reference/user-config/), which names variables instead of holding secrets.

## cuilo

| Variable                          | What it does                                                                                                                                                                                                                                                                                                   |
| --------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `CUILO_CONFIG_DIR`                | The folder for your user config, `config.toml`, and `trusted-projects.toml`, the projects you answered the trust question for.                                                                                                                                                                                 |
| `CUILO_CACHE_DIR`                 | The folder for downloaded packs (`packs/`), the copies `--verify` checks (`verify/`), and what the last update check found (`update.json`).                                                                                                                                                                    |
| `CUILO_UPDATES`                   | `auto`, `notify` or `off`, in place of `updates` in your user config: what cuilo does about new releases. See [automatic updates](/install/#automatic-updates). Any letter case works, and empty counts as unset. cuilo ignores any other value, so the config applies, and warns about it at most once a day. |
| `CI`                              | When set to a value that is not empty, cuilo never looks for new releases and prints no update notices.                                                                                                                                                                                                        |
| `NO_COLOR`                        | Turns colors off. Set to any value, even an empty one. Colors are also off when output is not a terminal, and with `--json`.                                                                                                                                                                                   |
| The variable in `auth.<host>.env` | The token cuilo sends when it downloads a pack from that host, e.g. `GITHUB_TOKEN`. Unset or empty sends no token; a token with spaces or control characters is an error.                                                                                                                                      |
| `GIT_SSH_COMMAND`, `GIT_SSH`      | When either is set, or git has `core.sshCommand`, cuilo leaves SSH to it for `git:` sources. Otherwise it runs `ssh -o BatchMode=yes -o ConnectTimeout=30`.                                                                                                                                                    |
| `PATH`                            | Where cuilo finds `git`, `ast-grep`, package managers, the checks `--verify` runs, and formatters from your system, such as rustfmt, gofmt, and Ruff or Black without a `.venv`. On Windows, cuilo also reads `PATHEXT` to find shims such as `npm.cmd`.                                                       |

An empty `CUILO_CONFIG_DIR` or `CUILO_CACHE_DIR` counts as unset. Without them, cuilo uses the system's folders:

| Folder | macOS                                            | Linux                                            | Windows                |
| ------ | ------------------------------------------------ | ------------------------------------------------ | ---------------------- |
| Config | `$XDG_CONFIG_HOME/cuilo`, else `~/.config/cuilo` | `$XDG_CONFIG_HOME/cuilo`, else `~/.config/cuilo` | `%APPDATA%\cuilo`      |
| Cache  | `~/Library/Caches/cuilo`                         | `$XDG_CACHE_HOME/cuilo`, else `~/.cache/cuilo`   | `%LOCALAPPDATA%\cuilo` |

`~` is `$HOME`. So cuilo also reads `HOME`, `XDG_CONFIG_HOME`, `XDG_CACHE_HOME`, `APPDATA` and `LOCALAPPDATA`.

:::caution
`--verify` needs a cache folder that only you can open, with no folder above it that every user can write to. A cache in `/tmp` is refused. If yours is shared, set `CUILO_CACHE_DIR` to a folder of your own and run `chmod 700` on it.
:::

### Proxies and certificates

cuilo's HTTPS client and certificate loader read these, for pack downloads, `cuilo upgrade` and update checks:

| Variable                                                            | What it does                                                                                     |
| ------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------ |
| `ALL_PROXY`, `HTTPS_PROXY`, `HTTP_PROXY`, or the same in lower case | An HTTP proxy for downloads. The first valid one, in that order, is used.                        |
| `NO_PROXY`, `no_proxy`                                              | Hosts that skip the proxy.                                                                       |
| `SSL_CERT_FILE`, `SSL_CERT_DIR`                                     | PEM certificates to trust. When either is set, cuilo trusts only these, instead of the system's. |

### Set for the programs cuilo runs

- The checks `--verify` runs get `CI=1` and a `CARGO_TARGET_DIR` inside the cuilo cache.
- `git` for `git:` sources runs with `GIT_TERMINAL_PROMPT=0`, an empty `GIT_ASKPASS` and `SSH_ASKPASS_REQUIRE=never`, so it never prompts.

## Install scripts

`install.sh` (macOS and Linux) and `install.ps1` (Windows) read:

| Variable                    | What it does                                                                                                                                                                                                                             |
| --------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `CUILO_VERSION`             | The version to install, e.g. `2.0.0`; a leading `v` is dropped. An argument to `install.sh` or `-Version` for `install.ps1` takes its place. Default: the newest stable release, or the newest pre-release while there is no stable one. |
| `CUILO_INSTALL`             | The install folder. cuilo goes into its `bin` folder. Default: `~/.cuilo`, or `~\.cuilo` on Windows.                                                                                                                                     |
| `CUILO_REQUIRE_SIGNATURE=1` | Refuse to install unless `minisign` is on `PATH` to check the signature. Without it, the scripts check the SHA-256 and also the signature when `minisign` is there.                                                                      |
| `CUILO_NO_MODIFY_PATH=1`    | `install.ps1` only: leave your user `Path` alone. By default it adds the `bin` folder.                                                                                                                                                   |

`install.sh` also reads `HOME` for the default folder and `PATH` to tell you whether to add the `bin` folder to it. `install.ps1` reads `PROCESSOR_ARCHITEW6432` or `PROCESSOR_ARCHITECTURE` to pick the build. See [install](/install/).
