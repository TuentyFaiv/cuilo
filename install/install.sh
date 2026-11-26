#!/bin/sh
# Installs cuilo from https://dl.cuilo.dev
#
#   curl --proto '=https' --tlsv1.2 -fsSL https://dl.cuilo.dev/install.sh | sh
#   curl --proto '=https' --tlsv1.2 -fsSL https://dl.cuilo.dev/install.sh | sh -s -- 2.0.0
#
# Environment:
#   CUILO_VERSION             the version (default: the newest stable release, or the newest
#                               pre-release while there is no stable one)
#   CUILO_INSTALL             the folder (default ~/.cuilo)
#   CUILO_REQUIRE_SIGNATURE=1 refuse to install unless minisign checks the signature
#
# Everything runs inside `main`, so a download cut short never runs half of this script.
set -eu

download_url="https://dl.cuilo.dev"
repo="TuentyFaiv/cuilo"
release_key="RWTFcgVmt+E7Xi77vi6Q/Yu68TJDTsBTKYHqOmyjSOY4VCgpEFCwl+dK"

fail() {
	printf 'error: %s\n' "$1" >&2
	exit 1
}

# HTTPS only, TLS 1.2 or newer, no redirects.
download() {
	curl --proto '=https' --tlsv1.2 --fail --silent --show-error --output "$2" "$1" ||
		fail "could not download $1"
}

is_version() {
	printf '%s\n' "$1" | grep -Eqx '[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?'
}

# The version a channel file names, or nothing when the channel has no release yet.
channel() {
	if curl --proto '=https' --tlsv1.2 --fail --silent --max-filesize 1024 --output "$tmp/$1" "$download_url/$1"; then
		head -n 1 "$tmp/$1" | tr -d '[:space:]'
	fi
}

sha256() {
	if command -v sha256sum >/dev/null 2>&1; then
		sha256sum "$1" | awk '{ print $1 }'
	elif command -v shasum >/dev/null 2>&1; then
		shasum -a 256 "$1" | awk '{ print $1 }'
	else
		fail "sha256sum or shasum is required to check the download"
	fi
}

main() {
	version="${CUILO_VERSION:-}"
	if [ $# -gt 0 ]; then
		version="$1"
	fi
	version="${version#v}"
	install_dir="${CUILO_INSTALL:-$HOME/.cuilo}"
	require_signature="${CUILO_REQUIRE_SIGNATURE:-0}"

	case "$(uname -s)" in
	Darwin) os="apple-darwin" ;;
	Linux) os="unknown-linux-musl" ;;
	*) fail "this script installs cuilo on macOS and Linux; on Windows, use install.ps1" ;;
	esac
	case "$(uname -m)" in
	x86_64 | amd64) arch="x86_64" ;;
	arm64 | aarch64) arch="aarch64" ;;
	*) fail "cuilo has no build for $(uname -m)" ;;
	esac
	# An x86_64 shell under Rosetta still gets the native Apple silicon build.
	if [ "$os" = "apple-darwin" ] && [ "$arch" = "x86_64" ] && [ "$(sysctl -n sysctl.proc_translated 2>/dev/null || true)" = "1" ]; then
		arch="aarch64"
	fi
	target="$arch-$os"
	binary="cuilo-$target"

	command -v curl >/dev/null 2>&1 || fail "curl is required"
	tmp="$(mktemp -d)"
	trap 'rm -rf "$tmp"' EXIT INT TERM

	if [ -z "$version" ]; then
		version="$(channel latest)"
		if [ -z "$version" ]; then
			version="$(channel latest-pre)"
			[ -n "$version" ] || fail "could not find the latest release at $download_url"
			printf 'No stable release yet; installing the pre-release %s.\n' "$version"
		fi
	fi
	is_version "$version" || fail "\`$version\` is not a cuilo version"
	base="$download_url/v$version"

	download "$base/$binary" "$tmp/$binary"
	download "$base/SHA256SUMS" "$tmp/SHA256SUMS"
	expected="$(awk -v name="$binary" '$2 == name || $2 == "*" name { print $1; exit }' "$tmp/SHA256SUMS")"
	[ -n "$expected" ] || fail "SHA256SUMS does not list $binary"
	actual="$(sha256 "$tmp/$binary")"
	[ "$actual" = "$expected" ] || fail "$binary does not match SHA256SUMS (expected $expected, got $actual)"

	if [ -n "$release_key" ] && command -v minisign >/dev/null 2>&1; then
		download "$base/$binary.minisig" "$tmp/$binary.minisig"
		signed="$(minisign -V -Q -P "$release_key" -m "$tmp/$binary" 2>/dev/null)" ||
			fail "$binary is not signed by the cuilo release key"
		[ "$signed" = "cuilo $version $binary" ] ||
			fail "$binary is signed as \`$signed\`, not as cuilo $version $binary"
		checked="signature and SHA-256 checked"
	elif [ "$require_signature" = "1" ]; then
		if [ -z "$release_key" ]; then
			fail "this install script has no release key, so it cannot check the signature"
		fi
		fail "CUILO_REQUIRE_SIGNATURE=1 needs minisign (https://jedisct1.github.io/minisign/)"
	else
		checked="SHA-256 checked; install minisign to check the signature too"
	fi

	mkdir -p "$install_dir/bin"
	chmod 755 "$tmp/$binary"
	mv -f "$tmp/$binary" "$install_dir/bin/cuilo"
	printf '{\n  "installer": "install.sh",\n  "version": "%s",\n  "target": "%s"\n}\n' "$version" "$target" \
		>"$install_dir/bin/cuilo-install.json"

	installed="$("$install_dir/bin/cuilo" --version)" || fail "the installed binary does not run"
	printf 'Installed %s to %s/bin (%s).\n' "$installed" "$install_dir" "$checked"
	# shellcheck disable=SC2016
	case ":$PATH:" in
	*":$install_dir/bin:"*) ;;
	*) printf 'Add it to your PATH, e.g. in ~/.zshrc or ~/.bashrc:\n  export PATH="%s/bin:$PATH"\n' "$install_dir" ;;
	esac
	printf 'Check where it was built with:\n  gh attestation verify %s/bin/cuilo --repo %s\n' "$install_dir" "$repo"
}

main "$@"
