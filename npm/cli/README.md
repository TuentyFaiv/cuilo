# @cuilo/cli

The npm package for [cuilo](https://cuilo.dev): generate projects and code from versioned, updatable packs, for humans and AI agents.

```sh
npm install --global @cuilo/cli
cuilo --help
```

The package has no dependencies and runs nothing at install time. Your package manager installs the one platform package that matches your system (for example `@cuilo/cli-darwin-arm64`), which holds the native binary. Every version is published from GitHub Actions with [npm provenance](https://docs.npmjs.com/generating-provenance-statements).

If `cuilo` reports that no binary is installed, your package manager skipped optional dependencies. Reinstall without `--omit=optional` / `--no-optional`, or use another install method from the docs.
