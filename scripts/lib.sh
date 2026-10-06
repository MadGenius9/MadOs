# shellcheck shell=sh
# Shared helpers for MadOS build scripts. Source, don't execute.

ROOT=$(cd "$(dirname "$0")/.." && pwd)
OUT="$ROOT/out"
export ROOT OUT

log() { printf '%s: %s\n' "${0##*/}" "$*" >&2; }
die() { log "error: $*"; exit 1; }

# shellcheck source=image/config.env
. "$ROOT/image/config.env"
eval "$(python3 "$ROOT/scripts/product.py" env)"

# Rootful podman is required: image-builder runs privileged and reads the
# image from root's container storage. We never escalate silently.
if [ "$(id -u)" -eq 0 ]; then
    SUDO=""
else
    SUDO="sudo"
fi
PODMAN="$SUDO podman"
export PODMAN

need() {
    command -v "$1" >/dev/null 2>&1 || die "'$1' not found; run 'make setup' for instructions"
}

announce_sudo() {
    if [ -n "$SUDO" ]; then
        log "this step needs root podman; you may be prompted for your password by sudo"
    fi
}

base_ref() {
    if [ -n "${BASE_IMAGE_DIGEST:-}" ]; then
        printf '%s@%s\n' "$BASE_IMAGE" "$BASE_IMAGE_DIGEST"
    else
        printf '%s:%s\n' "$BASE_IMAGE" "$FEDORA_VERSION"
    fi
}

git_commit() {
    if git -C "$ROOT" rev-parse --short=12 HEAD >/dev/null 2>&1; then
        c=$(git -C "$ROOT" rev-parse --short=12 HEAD)
        if [ -n "$(git -C "$ROOT" status --porcelain 2>/dev/null)" ]; then
            c="$c-dirty"
        fi
        printf '%s\n' "$c"
    else
        printf 'unknown\n'
    fi
}

# Build ID: UTC date/time plus source commit, e.g. 20261006.1412-3f2a9c1e5b7d
build_id() {
    printf '%s-%s\n' "$(date -u +%Y%m%d.%H%M)" "$(git_commit)"
}

sha256_file() {
    (cd "$(dirname "$1")" && sha256sum "$(basename "$1")" > "$(basename "$1").sha256")
    log "checksum: $(cat "$1.sha256")"
}
