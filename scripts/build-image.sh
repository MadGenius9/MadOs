#!/bin/sh
# make image: builds the MadOS bootable container image with podman.
#   MADOS_VARIANT=dev|release (default dev)
set -eu
# shellcheck source=scripts/lib.sh
. "$(dirname "$0")/lib.sh"
need podman
need python3

variant=${MADOS_VARIANT:-dev}
case $variant in dev|release) ;; *) die "MADOS_VARIANT must be dev or release" ;; esac
bid=$(build_id)
base=$(base_ref)
tag="$IMAGE_NAME:$MADOS_VERSION-$variant"

python3 "$ROOT/scripts/product.py" validate >/dev/null || die "product/product.toml is invalid"
announce_sudo
log "building $tag (base $base, build $bid)"
$PODMAN build \
    --file "$ROOT/image/Containerfile" \
    --build-arg "FEDORA_VERSION=$FEDORA_VERSION" \
    --build-arg "BUILDER_IMAGE=$BUILDER_IMAGE" \
    --build-arg "BASE_REF=$base" \
    --build-arg "MADOS_VARIANT=$variant" \
    --build-arg "GIT_COMMIT=$(git_commit)" \
    --build-arg "BUILD_ID=$bid" \
    --build-arg "PRODUCT_NAME=$MADOS_PRODUCT_NAME" \
    --build-arg "PRODUCT_VERSION=$MADOS_VERSION" \
    --tag "$tag" \
    --tag "$IMAGE_NAME:latest" \
    "$ROOT"

mkdir -p "$OUT"
printf '%s\n' "$tag" > "$OUT/image-ref"
printf '%s\n' "$bid" > "$OUT/build-id"
log "built $tag"
