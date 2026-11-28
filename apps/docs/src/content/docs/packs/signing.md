---

title: Sign a pack
description: Sign packs with minisign, trust keys in your user config, and see how cuilo.lock holds later versions to the same key.
---

A pack can carry a [minisign](https://jedisct1.github.io/minisign/) signature in `pack.minisig` at its root. The signature covers one line of text with the pack's name, version and digest, so it proves who published exactly these files. Official packs ship inside cuilo and need no signature.

## Sign a pack

1. Create a key pair once, and keep the secret key out of the pack:

   ```sh
   minisign -G -p acme.pub -s acme.key
   ```

   The second line of `acme.pub` is the public key, e.g. `RWQ…`. Its first line names the key id, 16 hex digits.

2. Get the pack's digest:

   ```sh
   cuilo pack add ./my-pack --json
   ```

   The output has a `digest` field, e.g. `"digest": "sha256:78bc34d94a9b0207331fe92275cbfa9895eaf5d88425e6bb961af16ccc3a27e6"`. It is the SHA-256 of every file in the pack except `pack.minisig`, so adding the signature does not change it.

3. Write the message `cuilo pack <name> <version> <digest>`, ending with a newline, and sign it into the pack. Use your pack's name, version and full digest:

   ```sh
   printf 'cuilo pack acme-ui 0.1.1 %s\n' "sha256:78bc34d94a9b0207331fe92275cbfa9895eaf5d88425e6bb961af16ccc3a27e6" > message.txt
   minisign -S -s acme.key -m message.txt -x my-pack/pack.minisig
   ```

4. Add the public key to `signing.keys` in your user config, as shown in [trust keys](#trust-keys), and load the pack again to check it:

   ```sh
   cuilo pack add ./my-pack
   ```

   ```text
   Loaded acme-ui 0.1.1 from path:./my-pack (signed by key 1C507A396C8260A0)
   ```

Sign the same files you publish. The digest covers every file cuilo loads from the source, so sign from a clean checkout without build output or other untracked files in the pack folder. After you publish, `cuilo pack add <source>` should report your key.

:::caution
The message must end with a newline. cuilo also refuses legacy signatures made with `minisign -S -l`. Both fail with `the signature by key … does not match its files`.
:::

## Trust keys

Signatures are checked against keys in your [user config](/reference/user-config/#signing), `config.toml` in your cuilo config folder. Projects cannot add keys.

```toml
[signing]
require = true                 # refuse unsigned packs and packs signed by keys you have not trusted
keys = { acme = "RWQ…" }       # minisign public keys, by a name you choose
revoked = ["1C3B7AE36BE79191"] # key ids refused even when the signature is valid
```

What happens to a pack from any source other than cuilo itself:

| The pack                                       | `require = false` (default) | `require = true` |
| ---------------------------------------------- | --------------------------- | ---------------- |
| Has no `pack.minisig`                          | Loads, unsigned             | Refused          |
| Is signed by a key not in `signing.keys`       | Loads, unsigned             | Refused          |
| Is signed by a trusted key, and it matches     | Loads, signed               | Loads, signed    |
| Is signed by a trusted key, and does not match | Refused                     | Refused          |
| Is signed by a revoked key                     | Refused                     | Refused          |
| Has a `pack.minisig` that is not a signature   | Refused                     | Refused          |

`cuilo pack add` says which case applies, e.g. `(signed by key 1C507A396C8260A0)` or `(not signed by a trusted key)`. The refusals read:

```text
error: pack `acme-ui` is not signed, and your user config requires signed packs
error: pack `acme-ui`: signed by key 1C507A396C8260A0, which you have not trusted; add it to `signing.keys` in your user config
error: pack `acme-ui`: the signature by key 1C507A396C8260A0 does not match its files
error: pack `acme-ui`: signed by key 1C507A396C8260A0, which is revoked
```

## What the lockfile records

When you apply a plan from a signed pack, `cuilo.lock` records the whole public key that signed it, not only its key id:

```toml
[[pack]]
name = "acme-ui"
version = "0.1.1"
source = "path:./my-pack"
digest = "sha256:78bc34d94a9b0207331fe92275cbfa9895eaf5d88425e6bb961af16ccc3a27e6"
signer = "RWSgYIJsOXpQHOL1uk5LU0m+qUBC+rigrbwDQeehQZgEH6eReZqpmZ9w"
```

From then on, `cuilo add`, `cuilo adopt` and `cuilo update` accept that pack only when the same key signed it. A later version signed by another key is refused, even if you trust both keys, and so is a version without a trusted signature:

```text
error: pack `acme-ui` was signed by the key RWSgYIJs…, but this version is signed by the key RWTv1kuB…
error: pack `acme-ui` was signed by the key RWSgYIJs…, but this version is not signed by a trusted key
```

A pack pinned without a signer can move to a signed version. See the [lockfile reference](/reference/lockfile/).

## Keys that share an id

A signature names its key only by the 8-byte key id, and any key can carry any id. That is why the lockfile records the whole key. For the same reason, two trusted keys with one key id are a config error, and every command that opens a project stops with:

```text
error: `signing.keys.other` and `signing.keys.acme` are different keys with the same key id 4F16435DED533568; keep only the one you meant to trust
```

The same key under two names is fine.

## Revoke a key

Add its key id, as minisign prints it, to `signing.revoked` in your user config. Letter case does not matter. cuilo then refuses every pack signed by that key, even when you trust it and the signature is valid. cuilo also refuses the key ids on its own built-in revocation list.
