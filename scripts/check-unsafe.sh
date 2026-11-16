#!/usr/bin/env bash
# Fails if the `unsafe` keyword appears in library code outside src/raw/ or
# anywhere in the rs-toml-macros crate. The crate root is
# #![deny(unsafe_code)] with #[allow(unsafe_code)] only on `mod raw`, and the
# macros crate is #![forbid(unsafe_code)]; this script guards those
# attributes themselves from being relaxed.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

status=0

hits="$(grep -rnwE 'unsafe' --include='*.rs' src macros/src | grep -v '^src/raw/' || true)"
if [ -n "$hits" ]; then
  echo "error: \`unsafe\` outside src/raw/:" >&2
  echo "$hits" >&2
  status=1
fi

if ! grep -qxF '#![deny(unsafe_code)]' src/lib.rs; then
  echo "error: src/lib.rs must keep #![deny(unsafe_code)]" >&2
  status=1
fi
allows="$(grep -rn 'allow(unsafe_code)' --include='*.rs' src macros/src || true)"
if [ "$(printf '%s\n' "$allows" | grep -c .)" -ne 1 ] \
  || ! grep -A1 -xF '#[allow(unsafe_code)]' src/lib.rs | grep -qxF 'mod raw;'; then
  echo "error: #[allow(unsafe_code)] is only permitted on \`mod raw\` in src/lib.rs:" >&2
  echo "$allows" >&2
  status=1
fi
if ! grep -qxF '#![forbid(unsafe_code)]' macros/src/lib.rs; then
  echo "error: macros/src/lib.rs must keep #![forbid(unsafe_code)]" >&2
  status=1
fi

if [ "$status" -eq 0 ]; then
  count="$(grep -rnwE 'unsafe' --include='*.rs' src/raw | wc -l | tr -d ' ')"
  echo "check-unsafe: ok ($count \`unsafe\` occurrences, all in src/raw/)"
fi
exit "$status"
