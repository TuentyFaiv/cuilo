#!/usr/bin/env node
/**
 * Runs the cuilo binary from the platform package the package manager installed; nothing is
 * downloaded or run at install time.
 */
import { spawnSync } from "node:child_process";
import { createRequire } from "node:module";
import process from "node:process";

const require = createRequire(import.meta.url);
const { name } = require("../package.json");

/** Whether this Linux uses musl instead of glibc. */
function isMusl() {
	if (process.platform !== "linux") {
		return false;
	}
	try {
		return !process.report?.getReport()?.header?.glibcVersionRuntime;
	} catch {
		return false;
	}
}

/** Platform package suffixes to try, in order. */
function platforms() {
	const base = `${process.platform}-${process.arch}`;
	if (process.platform !== "linux") {
		return [base];
	}
	return isMusl() ? [`${base}-musl`, base] : [base, `${base}-musl`];
}

/** The path of the installed binary, or `undefined`. */
function binary() {
	const file = process.platform === "win32" ? "bin/cuilo.exe" : "bin/cuilo";
	for (const platform of platforms()) {
		try {
			return require.resolve(`${name}-${platform}/${file}`);
		} catch {
			// Not installed for this platform; try the next.
		}
	}
}

const path = binary();
if (!path) {
	process.stderr.write(
		`error: ${name} has no binary for ${process.platform}-${process.arch}. ` +
			"Your package manager may have skipped optional dependencies (for example with --omit=optional or " +
			"--no-optional); reinstall without that flag, or use the install script from https://cuilo.dev.\n"
	);
	process.exit(1);
}

const result = spawnSync(path, process.argv.slice(2), { stdio: "inherit" });
if (result.error) {
	process.stderr.write(`error: could not run ${path}: ${result.error.message}\n`);
	process.exit(1);
}
if (result.signal) {
	process.kill(process.pid, result.signal);
}
process.exit(result.status ?? 1);
