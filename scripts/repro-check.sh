#!/bin/sh
# Reproducibility check for MadOS-generated content: stages the system tree
# twice from the same inputs (same binaries, same SOURCE_DATE_EPOCH) and
# requires byte-identical results. Catches timestamps, ordering and other
# nondeterminism in scripts/stage-system.py. (Full image reproducibility is
# a later milestone; see docs/development/testing.md.)
set -eu
# shellcheck source=scripts/lib.sh
. "$(dirname "$0")/lib.sh"
bin=${1:-$ROOT/target/release}
[ -x "$bin/madosctl" ] || die "no release binaries in $bin; run 'make build-release'"
SOURCE_DATE_EPOCH=$(git -C "$ROOT" log -1 --format=%ct 2>/dev/null || echo 0)
export SOURCE_DATE_EPOCH
work="$OUT/repro"
rm -rf "$work"
for n in 1 2; do
    python3 "$ROOT/scripts/stage-system.py" --bin-dir "$bin" --destdir "$work/$n" \
        --build-id repro --git-commit "$(git_commit)" --base-image repro >/dev/null
done
(cd "$work/1" && find . -type f -print0 | sort -z | xargs -0 sha256sum) > "$work/1.sha256"
(cd "$work/2" && find . -type f -print0 | sort -z | xargs -0 sha256sum) > "$work/2.sha256"
if diff -u "$work/1.sha256" "$work/2.sha256"; then
    log "staged tree is reproducible ($(wc -l < "$work/1.sha256") files)"
else
    die "staged tree differs between identical builds"
fi
