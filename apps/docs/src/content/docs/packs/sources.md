---

title: Pack sources
description: Every place cuilo can load a pack from, what cuilo.lock pins, namespaces in cuilo.json, and credentials for private sources.
---

A source is one string that says where a pack comes from. Pass it to `--from` on `cuilo add`, `cuilo adopt` and `cuilo update`, or check it first with `cuilo pack add <source>`, which loads and verifies the pack without changing the project.

## Source forms

| Source                     | Example                                                                             |
| -------------------------- | ----------------------------------------------------------------------------------- |
| Official, built into cuilo | `ui`, `official:backend`                                                            |
| Folder                     | `./packs/mine`, `../shared/ui`, `path:/opt/packs/mine`                              |
| GitHub tag or commit       | `gh:acme/packs@v1.2.0`, `gh:acme/packs/ui@v1.2.0`                                   |
| HTTPS tarball              | `https://example.com/ui-1.2.0.tar.gz`                                               |
| Any git repository, SSH    | `git:git@github.com:acme/packs.git@v1.2.0`, `git:ssh://git@host/x.git//packs/ui@v2` |
| Registry item              | `registry:https://acme.dev/r/card.json`, `registry:@acme/card`                      |
| Namespace from the project | `@acme/ui`                                                                          |

### Official

`ui` and `backend` ship inside the cuilo binary, so they load without a download, and a newer cuilo brings their next version. A bare name is read as an official pack; `official:<name>` says so explicitly. See [ui](/directory/ui/) and [backend](/directory/backend/).

### Folder

A source that starts with `./`, `../`, `/` or `.\` is a folder, and so is anything after `path:`. Relative folders are read from the project root. This is how you [test a pack](/packs/write-a-pack/#test-it-locally) while you write it.

### GitHub

`gh:owner/repo@ref` downloads the repository at a tag or commit. Add a folder after the repository when the pack is not at its root: `gh:acme/packs/ui@v1.2.0`. The `@ref` is required.

Without a token, cuilo downloads from `codeload.github.com`. With a token for `github.com` in your user config, it downloads through the GitHub API instead, which also works for private repositories.

### HTTPS tarball

An `https://` URL whose path ends in `.tar.gz` or `.tgz`. If the archive wraps every file in one top folder, as release archives do, cuilo removes that folder.

### Git over SSH

`git:` takes an `ssh://` URL or a `user@host:path` URL, then `@ref` with a tag or commit. Add `//folder` before the `@` to pick a folder inside the repository. This is how GitLab, Gitea and self-hosted repositories are reached.

cuilo runs your own `git`, so your SSH keys, agent and `~/.ssh/config` apply. It allows only the SSH protocol, fetches one commit and reads it with `git archive`, so nothing is checked out. It never prompts: unless you set your own SSH command with `GIT_SSH_COMMAND`, `GIT_SSH` or git's `core.sshCommand`, SSH runs with `BatchMode=yes` and a 30-second connection timeout. Each git command stops after two minutes. `git:` URLs over HTTPS are refused; use `gh:` or a tarball for those.

### Registry item

`registry:` loads an item in the `registry-item.json` format and turns it into a pack named `registry-<item>`. See [registry items](/packs/registry-items/).

### Namespace

`@namespace/pack` names a pack inside a source you set once in the project config:

```json
{
	"sources": {
		"@acme": "gh:acme/packs@v1.2.0"
	}
}
```

Then use `@acme/ui` wherever a source goes, e.g. `cuilo add button --from @acme/ui`.

A namespace points to a folder, `gh:`, `git:` or `https://` source. In it, cuilo looks for `packs/<pack>/pack.toml`; when there is none, it uses the pack at the source's root. The pack inside keeps its own name from its `pack.toml`, and it cannot be `ui` or `backend`. An unknown namespace fails with ``namespace `@acme` is not configured; add it to `sources` in the project config``. See [`sources`](/reference/configuration/#sources).

The lockfile records the source as `@acme/ui`. When you point the namespace at a new tag, the pinned pack no longer matches, so move to the new version with `cuilo update <pack> --from @acme/ui`.

## What cuilo.lock pins

`cuilo pack add` only loads and checks. When you apply a plan from a pack, `cuilo.lock` records it:

```toml
[[pack]]
name = "acme-ui"
version = "0.1.0"
source = "path:./my-pack"
digest = "sha256:7ed4cf6fb1364c68d53fc6d1a941d8e5ed266bf41cda4801d5d4bafd80ea8c69"
```

- `source` is the source in its full form: `./my-pack` becomes `path:./my-pack` and `ui` becomes `official:ui`. A folder given as an absolute path gets no `source`, since it only exists on your machine, so pass `--from` again to use it.
- `digest` is the SHA-256 of every file in the pack except its signature.
- `signer` is added when a trusted key signed the pack; see [signing](/packs/signing/).

Each command loads the packs the lockfile pins, so later commands such as `cuilo add acme-ui/card` find them without `--from`. Downloaded packs are kept in the `packs` folder of your cuilo cache by digest, so a pinned pack loads offline. If a source now serves other files for the same pin, cuilo refuses it:

```text
error: `path:./my-pack` changed: the lockfile pinned sha256:7ed4…, the source now has sha256:45ad…
```

To move to new files, raise the pack's version and run `cuilo update <pack> --from <source>`. A lockfile record loads only the pack and version it names. Official packs are not checked against a pinned digest: they ship with cuilo, so a newer cuilo brings their next version. See the [lockfile reference](/reference/lockfile/).

## Private sources

Credentials live in your [user config](/reference/user-config/#auth), `config.toml` in your cuilo config folder, never in the project config. A cloned project cannot send your tokens anywhere. The config stores only the name of an environment variable:

```toml
[auth."github.com"]
env = "GITHUB_TOKEN"
```

cuilo sends the token as `Authorization: Bearer <token>`. `header` changes the header and `scheme` the word before the token; an empty `scheme` sends the token alone:

```toml
[auth."gitlab.example.com"]
env = "GITLAB_TOKEN"
header = "PRIVATE-TOKEN"
scheme = ""
```

The key is the host of the download URL; use `github.com` for `gh:` sources. Tokens go only to that host, never to a host a download redirects to. When the variable is unset or empty, cuilo sends no header. When a download without a token gets HTTP 401, 403 or 404, the error suggests adding an `[auth."<host>"]` table, or says that the configured variable is not set. `git:` sources do not use this table; they use your SSH setup.

## Reserved names

The names `ui` and `backend` belong to the official packs and load only from cuilo itself. A pack with one of those names from any other source is refused:

```text
error: `path:./bad` is a pack named `ui`, the name of a pack built into cuilo; packs from other sources need another name
```

## Downloads

Downloads use HTTPS only, verified against the certificates your operating system trusts. cuilo follows at most 5 redirects, all to HTTPS, gives up after 60 seconds and stops at 64 MiB. Proxy and certificate variables are listed in [environment variables](/reference/environment/).

In a build of cuilo without the `remote` feature of `cuilo-packs`, which is on by default, remote sources load only from the cache and `git:` sources are refused.
