---

title: Firma un pack
description: Firma packs con minisign, confía en llaves desde tu configuración de usuario y ve cómo cuilo.lock exige la misma llave en versiones posteriores.
---

Un pack puede llevar una firma de [minisign](https://jedisct1.github.io/minisign/) en `pack.minisig`, en su raíz. La firma cubre una línea de texto con el nombre, la versión y el digest del pack, así que demuestra quién publicó exactamente estos archivos. Los packs oficiales vienen dentro de cuilo y no necesitan firma.

## Firma un pack

1. Crea un par de llaves una sola vez, y deja la llave secreta fuera del pack:

   ```sh
   minisign -G -p acme.pub -s acme.key
   ```

   La segunda línea de `acme.pub` es la llave pública, por ejemplo `RWQ…`. La primera línea indica el id de la llave, de 16 dígitos hexadecimales.

2. Obtén el digest del pack:

   ```sh
   cuilo pack add ./my-pack --json
   ```

   La salida tiene un campo `digest`, por ejemplo `"digest": "sha256:78bc34d94a9b0207331fe92275cbfa9895eaf5d88425e6bb961af16ccc3a27e6"`. Es el SHA-256 de todos los archivos del pack excepto `pack.minisig`, así que agregar la firma no lo cambia.

3. Escribe el mensaje `cuilo pack <name> <version> <digest>`, con un salto de línea al final, y fírmalo dentro del pack. Usa el nombre, la versión y el digest completo de tu pack:

   ```sh
   printf 'cuilo pack acme-ui 0.1.1 %s\n' "sha256:78bc34d94a9b0207331fe92275cbfa9895eaf5d88425e6bb961af16ccc3a27e6" > message.txt
   minisign -S -s acme.key -m message.txt -x my-pack/pack.minisig
   ```

4. Agrega la llave pública a `signing.keys` en tu configuración de usuario, como se muestra en [llaves de confianza](#llaves-de-confianza), y vuelve a cargar el pack para comprobarlo:

   ```sh
   cuilo pack add ./my-pack
   ```

   ```text
   Loaded acme-ui 0.1.1 from path:./my-pack (signed by key 1C507A396C8260A0)
   ```

Firma los mismos archivos que publicas. El digest cubre todos los archivos que cuilo carga del origen, así que firma desde un checkout limpio, sin resultados de compilación ni otros archivos sin seguimiento en la carpeta del pack. Después de publicar, `cuilo pack add <source>` debería mostrar tu llave.

:::caution
El mensaje tiene que terminar con un salto de línea. cuilo también rechaza las firmas con el formato antiguo, hechas con `minisign -S -l`. En los dos casos falla con `the signature by key … does not match its files`.
:::

## Llaves de confianza

Las firmas se comparan con las llaves de tu [configuración de usuario](/es-mx/reference/user-config/#signing), `config.toml` en tu carpeta de configuración de cuilo. Los proyectos no pueden agregar llaves.

```toml
[signing]
require = true                 # refuse unsigned packs and packs signed by keys you have not trusted
keys = { acme = "RWQ…" }       # minisign public keys, by a name you choose
revoked = ["1C3B7AE36BE79191"] # key ids refused even when the signature is valid
```

Qué pasa con un pack de cualquier origen que no sea el propio cuilo:

| El pack                                                  | `require = false` (por defecto) | `require = true`   |
| -------------------------------------------------------- | ------------------------------- | ------------------ |
| No tiene `pack.minisig`                                  | Carga como no firmado           | Se rechaza         |
| Está firmado por una llave que no está en `signing.keys` | Carga como no firmado           | Se rechaza         |
| Está firmado por una llave de confianza y coincide       | Carga como firmado              | Carga como firmado |
| Está firmado por una llave de confianza y no coincide    | Se rechaza                      | Se rechaza         |
| Está firmado por una llave revocada                      | Se rechaza                      | Se rechaza         |
| Tiene un `pack.minisig` que no es una firma              | Se rechaza                      | Se rechaza         |

`cuilo pack add` indica qué caso aplica, por ejemplo `(signed by key 1C507A396C8260A0)` o `(not signed by a trusted key)`. Los rechazos dicen:

```text
error: pack `acme-ui` is not signed, and your user config requires signed packs
error: pack `acme-ui`: signed by key 1C507A396C8260A0, which you have not trusted; add it to `signing.keys` in your user config
error: pack `acme-ui`: the signature by key 1C507A396C8260A0 does not match its files
error: pack `acme-ui`: signed by key 1C507A396C8260A0, which is revoked
```

## Qué registra el lockfile

Cuando aplicas un plan de un pack firmado, `cuilo.lock` registra la llave pública completa que lo firmó, no solo su id:

```toml
[[pack]]
name = "acme-ui"
version = "0.1.1"
source = "path:./my-pack"
digest = "sha256:78bc34d94a9b0207331fe92275cbfa9895eaf5d88425e6bb961af16ccc3a27e6"
signer = "RWSgYIJsOXpQHOL1uk5LU0m+qUBC+rigrbwDQeehQZgEH6eReZqpmZ9w"
```

A partir de ahí, `cuilo add`, `cuilo adopt` y `cuilo update` solo aceptan ese pack cuando lo firmó la misma llave. Una versión posterior firmada por otra llave se rechaza, aunque confíes en las dos, y lo mismo pasa con una versión que no tiene una firma de confianza:

```text
error: pack `acme-ui` was signed by the key RWSgYIJs…, but this version is signed by the key RWTv1kuB…
error: pack `acme-ui` was signed by the key RWSgYIJs…, but this version is not signed by a trusted key
```

Un pack fijado sin firmante sí puede pasar a una versión firmada. Consulta la [referencia del lockfile](/es-mx/reference/lockfile/).

## Llaves que comparten id

Una firma identifica su llave solo por el id de 8 bytes, y cualquier llave puede llevar cualquier id. Por eso el lockfile registra la llave completa. Por la misma razón, dos llaves de confianza con el mismo id son un error de configuración, y cualquier comando que abre un proyecto se detiene con:

```text
error: `signing.keys.other` and `signing.keys.acme` are different keys with the same key id 4F16435DED533568; keep only the one you meant to trust
```

La misma llave con dos nombres no es problema.

## Revoca una llave

Agrega su id, tal como lo muestra minisign, a `signing.revoked` en tu configuración de usuario. No importan las mayúsculas o minúsculas. Desde ese momento, cuilo rechaza todos los packs firmados con esa llave, aunque confíes en ella y la firma sea válida. cuilo también rechaza los ids de su propia lista de revocación integrada.
