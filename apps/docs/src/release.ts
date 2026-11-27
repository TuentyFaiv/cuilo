import cli from "../../../npm/cli/package.json" with { type: "json" };

/** cuilo's version, from the npm wrapper, which the release check keeps equal to the Cargo workspace's. */
export const { version } = cli;

/** Whether the version is a pre-release, which Cargo installs only when asked for by version. */
export const prerelease = version.includes("-");

/** Installs the current release's prebuilt binary with `cargo binstall`. */
export const cargoBinstall = prerelease ? `cargo binstall cuilo --version ${version}` : "cargo binstall cuilo";

/** Builds and installs the current release from crates.io. */
export const cargoInstall = prerelease
	? `cargo install cuilo --locked --version ${version}`
	: "cargo install cuilo --locked";
