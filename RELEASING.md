# Releasing cuilo

A release starts when a maintainer pushes a `v<version>` tag that matches the Cargo workspace version.
`.github/workflows/release.yml` then:

1. **check**: fails unless the tag equals `cargo xtask version`, and runs `cargo xtask release-check`. That
   check fails while the channels disagree (npm name, download host, release key, target list) or while something
   still blocks a release (no release key, or a reserved `.invalid` download host).
2. **build**: builds `cuilo-<target>[.exe]` for every target in `crates/xtask/src/release.rs` with
   `--profile dist`, and runs each native binary once.
3. **release** (waits for the `release` environment): writes `SHA256SUMS`, attests every binary with GitHub
   artifact attestations, and signs `SHA256SUMS`, every binary and both install scripts with minisign. Each
   signature's trusted comment names the version and file (`cuilo 2.0.0 cuilo-x86_64-pc-windows-msvc.exe`), so
   a signature can't be reused for another file or version. Then it publishes the GitHub Release (a pre-release for
   `-alpha`, `-beta`, …), which is the mirror and the home of the attestations.
4. **cloudflare** (waits for `release`): uploads the version to the R2 bucket under `v<version>/`, never replacing
   a file already there. Then it moves the channels (`latest-pre` for every newer version, `latest` only for
   stable ones) and the root install scripts forward. Last, it installs the release through the public URL with
   `CUILO_REQUIRE_SIGNATURE=1`.
5. **npm**, **crates**, **homebrew** (each waits for `release`): publish the platform packages and then the
   wrapper with provenance, publish the crates, and open a pull request with the new formula on the tap. The formula
   points at the Cloudflare files. All nine npm packages get one dist-tag: `latest`, or `next` for a pre-release once
   npm's `latest` is a stable release (the `0.0.0` placeholders don't count).

## The download host

```text
https://<download host>/
  install.sh, install.ps1 (+ .minisig)    newest scripts; cached 5 minutes
  latest                                  newest stable version; cached 1 minute
  latest-pre                              newest version, pre-releases included
  v2.0.0/                                 one folder per version, never changed
    cuilo-<target>[.exe] (+ .minisig)
    SHA256SUMS, SHA256SUMS.minisig
    install.sh, install.ps1 (+ .minisig)  the scripts as they were for this version
```

## What protects users

| Threat                                             | What stops it                                                                                                                                                                                                                                        |
| -------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Someone on the network reads or changes a download | HTTPS only, TLS 1.2 or newer, no redirects: `curl --proto '=https' --tlsv1.2`, PowerShell with TLS 1.2 and `-MaximumRedirection 0`, cuilo's own HTTPS client. The domain adds HSTS, DNSSEC and CAA records (setup below).                            |
| A changed or swapped binary on the host            | Every binary is signed with a key the host never sees. `cuilo upgrade`, automatic updates and `cargo binstall` require the signature, and the install scripts check it when `minisign` is installed (`CUILO_REQUIRE_SIGNATURE=1` makes it required). |
| An old, vulnerable release offered as the newest   | `cuilo upgrade` and automatic updates never go down, and each signature names its version, so an old signed file can't stand in for a new one.                                                                                                       |
| A published version changed later                  | The R2 bucket lock on `v` and the workflow's never-replace upload; immutable GitHub Releases; npm and crates.io never accept the same version twice.                                                                                                 |
| A leaked CI credential                             | Every publishing job waits for your approval of the `release` environment, and each token can reach one bucket, tap or package and nothing else.                                                                                                     |
| A half-downloaded `curl … \| sh`                   | install.sh runs everything inside `main`, so a cut-off script runs nothing.                                                                                                                                                                          |

What remains: the first `curl … | sh` trusts the download host over HTTPS, as rustup, Bun and Deno do. A user who
wants more can download `install.sh` and `install.sh.minisig`, check them with
`minisign -V -P <key> -m install.sh`, and run the script with `CUILO_REQUIRE_SIGNATURE=1`. Or they can use npm
(provenance) or Homebrew (pinned digests). Windows and macOS binaries are not code-signed with Authenticode or
Apple notarization yet; that takes paid certificates.

## One-time setup (owner)

- [x] **Release key.** On your machine: `minisign -G -W -p release.pub -s release.key` (unencrypted, because
  the CI secret is the protection). Put the key line of `release.pub` (the second line) into
  `crates/cuilo/release-key.pub`, `release_key="…"` in `install/install.sh`, `$ReleaseKey = '…'` in
  `install/install.ps1` and `pubkey = "…"` in `crates/cuilo/Cargo.toml`; `cargo xtask release-check` lists any you
  missed. Publish the key in the README and docs too, so users can check it against more than one place. Keep
  `release.key` offline as a backup.
- [ ] **Cloudflare domain.** Add the domain to Cloudflare, then:
  - turn on DNSSEC and add its DS record at the registrar;
  - under SSL/TLS, turn on Always Use HTTPS, set the minimum TLS version to 1.2, turn on TLS 1.3, and enable HSTS
    (max-age 1 year, include subdomains, preload). `.dev` and `.app` domains are HTTPS-only for browsers already;
  - add CAA records that allow only the certificate authorities Cloudflare uses, plus an `iodef` address to hear
    about refused requests;
  - protect the Cloudflare account with a hardware security key, and keep its members to the people who release.
- [ ] **R2 bucket.** Create a bucket (e.g. `cuilo-releases`), connect the custom domain `dl.cuilo.dev`
  under Settings → Public access, and leave the `r2.dev` URL off. Add a bucket lock rule for the prefix `v` with
  indefinite retention, so no published version can be changed or deleted. Add a Cache Rule for that hostname
  (eligible for cache, respect origin headers); the workflow sets `Cache-Control` on every file.
- [ ] **R2 token.** Create an R2 API token with _Object Read & Write_ on that bucket only.
- [x] **Download host in the code.** `brand::DOWNLOAD_URL` is `https://dl.cuilo.dev`, with copies in
  `install/install.sh`, `install/install.ps1` and the `pkg-url` in `crates/cuilo/Cargo.toml`. If it changes,
  `cargo xtask release-check` lists any copy you missed.
- [ ] **`release` environment** (repository settings → Environments): required reviewer = you; deployment tags
  `v*` only. Secrets: `MINISIGN_SECRET_KEY` (the full `release.key` file), `R2_ACCOUNT_ID`, `R2_ACCESS_KEY_ID`,
  `R2_SECRET_ACCESS_KEY`, and `HOMEBREW_TAP_TOKEN` (a fine-grained token with _contents_ and _pull requests_
  write access to `tuentyfaiv/homebrew-tap` only). Variable: `R2_BUCKET`.
- [ ] **Immutable releases** (repository settings → General → Releases) so a published release cannot be edited.
- [ ] **Tag protection**: a ruleset that only lets maintainers create `v*` tags.
- [ ] **Homebrew tap**: create the public repository `tuentyfaiv/homebrew-tap`; the release job adds `Formula/cuilo.rb`
  through a pull request. `HOMEBREW_TAP_TOKEN` needs write access to its contents and pull requests.
- [ ] **npm**: create the free npm organization `cuilo`, which owns the `@cuilo` scope. Trusted publishing can only
  be set on a package that exists, so claim the nine names first with `0.0.0` placeholders: a `package.json` with
  the name, version `0.0.0`, a description, the license and the repository, plus a README saying the release
  workflow publishes the real package. Sign in with `npm login` and run `npm publish <dir> --access public` for each.
  Then, on npmjs.com, add the trusted publisher to each package (repository `TuentyFaiv/cuilo`, workflow
  `release.yml`, environment `release`), set its publishing access to require two-factor authentication and disallow
  tokens, and `npm logout`. The release workflow publishes every real version, as `latest` until 2.0 is stable.
- [ ] **crates.io**: likewise, claim `cuilo-core`, `cuilo-packs`, `cuilo-rpc`, `cuilo-mcp` and `cuilo` with `0.0.0`
  placeholders, each an empty library with its own `[workspace]` table. Create an API token that expires within a
  day, with only the `publish-new` and `yank` scopes, and run `cargo publish --allow-dirty` in each. Add the trusted
  publisher (workflow `release.yml`, environment `release`) to each crate, yank the placeholders
  (`cargo yank --version 0.0.0 <crate>`) so nothing installs them, and revoke the token.

- [ ] **v1 users**: once the first release is out, point the v1 package at the new one with
  `npm deprecate @tuentyfaiv/cli "Moved to @cuilo/cli (Cuilo v2): https://cuilo.dev"`.

## Each release

1. Bump `version` in the root `Cargo.toml` (`[workspace.package]` and the `cuilo-*` dependency pins) and
   `npm/cli/package.json`, and replace "unreleased" ("sin publicar" in es-MX) in the version's changelog headings
   with the release date; `cargo xtask release-check` lists anything missed. The version's section of the English
   changelog becomes the GitHub Release notes (`cargo xtask release-notes` prints them).
2. Run `cargo xtask ci`, review, commit and push.
3. Tag the reviewed commit (`git tag -s v<version> && git push origin v<version>`), then approve the `release`
   environment for each job. Once Cloudflare, npm and crates.io have the version, the workflow runs the install smoke
   test (`.github/workflows/install-smoke.yml`): fresh macOS, Ubuntu and Windows runners install it with the install
   script, npm, pnpm, Bun, Yarn and `cargo binstall` (and `cargo install` on Ubuntu), check `cuilo --version`, and
   plan a piece offline.
4. Merge the tap pull request, then run the smoke test again with Homebrew:
   `gh workflow run install-smoke.yml -f version=<version>`. It fails until the tap has that version.

A bad release, such as one the smoke test fails on, is never replaced: publish a fixed version. Until then, a channel
file can be pointed back at the previous version by hand. `cuilo upgrade` and automatic updates then stay where they
are, and the install scripts install that one.

## Verifying a release

```sh
key="$(cat crates/cuilo/release-key.pub)"
minisign -V -P "$key" -m cuilo-aarch64-apple-darwin   # prints the trusted comment: version and file
minisign -V -P "$key" -m SHA256SUMS
sha256sum --check --ignore-missing SHA256SUMS
gh attestation verify cuilo-aarch64-apple-darwin --repo TuentyFaiv/cuilo
```

The checksums alone only catch a corrupted download, because they come from the same host as the binaries. The
signature and the attestation are what show a release came from this repository's workflow.

To check later that a version still installs and runs through every channel, run the smoke test as in step 4 of
[Each release](#each-release).
