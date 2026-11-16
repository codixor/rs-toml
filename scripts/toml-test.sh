#!/usr/bin/env bash
# Runs the official toml-lang/toml-test suite against rs-toml through the
# decoder/encoder adapters in diver/toml-test-harness. Used locally and in
# .github/workflows/conformance.yml.
#
# Environment:
#   TOML_VERSIONS   space-separated spec versions to test (default "1.1 1.0")
#   TOML_TEST_BIN   use this toml-test binary instead of downloading one
#   TOML_TEST_CACHE download cache (default ${XDG_CACHE_HOME:-~/.cache}/rs-toml)
#   CARGO_TARGET_DIR  build directory (default <repo>/target/toml-test-harness)
#
# Each version uses diver/toml-test-harness/skip-<version>.txt with
# -skip-must-err: a skipped test that starts passing fails the run.
set -euo pipefail

TOML_TEST_VERSION="v2.2.0"

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
harness="$root/diver/toml-test-harness"

download_toml_test() {
	local os arch platform sha
	os="$(uname -s | tr '[:upper:]' '[:lower:]')"
	arch="$(uname -m)"
	case "$arch" in
	x86_64 | amd64) arch="amd64" ;;
	aarch64 | arm64) arch="arm64" ;;
	esac
	platform="$os-$arch"
	# SHA-256 of the release .gz assets for $TOML_TEST_VERSION.
	case "$platform" in
	linux-amd64) sha="08f9e0a97da1151c33debf01358a8f5ef45e2a56be201241ae5eb5c2e9323fef" ;;
	linux-arm64) sha="2f2e7f3e7cdbaa252bd6a3f1480f044b25aaf7a76353eb8f229ce8befa5a82b3" ;;
	darwin-amd64) sha="17e0365948ab7da54e0541bf22dce7dc809e407cd1e14bf78576cfbfd48ffee1" ;;
	darwin-arm64) sha="f36b1310b03a95dfa6b92ef535018db8ccc997ba20e79f3fd28d0f97c9174f35" ;;
	*)
		echo "no pinned toml-test binary for $platform; set TOML_TEST_BIN" >&2
		exit 2
		;;
	esac

	local cache="${TOML_TEST_CACHE:-${XDG_CACHE_HOME:-$HOME/.cache}/rs-toml}"
	local bin="$cache/toml-test-$TOML_TEST_VERSION-$platform"
	if [[ ! -x "$bin" ]]; then
		mkdir -p "$cache"
		local gz="$bin.gz.partial"
		curl -fsSL --retry 3 -o "$gz" \
			"https://github.com/toml-lang/toml-test/releases/download/$TOML_TEST_VERSION/toml-test-$TOML_TEST_VERSION-$platform.gz"
		local actual
		if command -v sha256sum >/dev/null 2>&1; then
			actual="$(sha256sum "$gz" | cut -d' ' -f1)"
		else
			actual="$(shasum -a 256 "$gz" | cut -d' ' -f1)"
		fi
		if [[ "$actual" != "$sha" ]]; then
			rm -f "$gz"
			echo "checksum mismatch for toml-test $platform: got $actual" >&2
			exit 1
		fi
		gunzip -c "$gz" >"$bin.partial"
		rm -f "$gz"
		chmod +x "$bin.partial"
		mv "$bin.partial" "$bin"
	fi
	printf '%s\n' "$bin"
}

toml_test="${TOML_TEST_BIN:-$(download_toml_test)}"
"$toml_test" version

export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$root/target/toml-test-harness}"
cargo build --release --locked --manifest-path "$harness/Cargo.toml"
# toml-test splits -decoder/-encoder on whitespace, so run from the build
# directory and pass relative paths (the checkout path may contain spaces).
cd "$CARGO_TARGET_DIR/release"
decoder="./decoder"
encoder="./encoder"

status=0
for version in ${TOML_VERSIONS:-1.1 1.0}; do
	skip_file="$harness/skip-$version.txt"
	skips=()
	if [[ -f "$skip_file" ]]; then
		# Strip comments and blank lines; first field is the test name.
		while read -r name _; do
			[[ -z "$name" || "$name" == \#* ]] && continue
			skips+=("-skip=$name")
		done <"$skip_file"
	fi
	echo "== toml-test $TOML_TEST_VERSION, TOML $version (${#skips[@]} skipped)"
	if ! "$toml_test" test -color=never -toml="$version" \
		-decoder="$decoder" -encoder="$encoder" \
		-skip-must-err "${skips[@]+"${skips[@]}"}"; then
		status=1
	fi
done
exit "$status"
