#!/usr/bin/env bash
#
# Print the DDS type name the C idlc registers for the codegen fixture IDL.
#
# The differential test in cyclonedds-test-suite (tests/typename_vs_idlc.rs) asserts the
# name the Rust codegen emits (via #[dds_typename]) against this. Run this after changing
# the fixture or when a new CycloneDDS release lands, and reconcile the transcribed value
# in that test.
#
# Unlike scripts/regen-ops-fixtures.sh, idlc is NOT built here: it is expected to already
# exist. The search order is $IDLC, then $CYCLONEDDS_HOME/bin/idlc[.exe], then idlc on PATH.
#
# Usage:
#   scripts/regen-typename-fixture.sh
#
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
idl_file="$repo_root/cyclonedds-test-suite/tests/idl/codegen/nested_modules.idl"

find_idlc() {
    if [ -n "${IDLC:-}" ] && [ -x "$IDLC" ]; then
        printf '%s\n' "$IDLC"
        return 0
    fi
    if [ -n "${CYCLONEDDS_HOME:-}" ]; then
        for candidate in "$CYCLONEDDS_HOME/bin/idlc.exe" "$CYCLONEDDS_HOME/bin/idlc"; do
            if [ -x "$candidate" ]; then
                printf '%s\n' "$candidate"
                return 0
            fi
        done
    fi
    if command -v idlc >/dev/null 2>&1; then
        command -v idlc
        return 0
    fi
    return 1
}

if [ ! -f "$idl_file" ]; then
    echo "fixture IDL not found: $idl_file" >&2
    exit 1
fi

idlc_bin="$(find_idlc)" || {
    echo "idlc not found; set IDLC or CYCLONEDDS_HOME" >&2
    exit 1
}

out_dir="$(mktemp -d)"
trap 'rm -rf "$out_dir"' EXIT

cp "$idl_file" "$out_dir/"
(
    cd "$out_dir"
    # The generator plugin (cycloneddsidl) is loaded from the loader path; it sits next to
    # the idlc binary in a normal install, so running the binary by absolute path suffices.
    "$idlc_bin" "$(basename "$idl_file")"
)

echo "==> idlc $("$idlc_bin" -v 2>&1 | tail -1)"
echo "==> registered type name(s)"
echo
grep -ho 'm_typename = "[^"]*"' "$out_dir"/*.c
