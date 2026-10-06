#!/bin/sh
# make iso: builds the EXPERIMENTAL installer ISO. Requires `make image`.
# Output: out/mados-<version>-<build>-installer.iso
set -eu
# shellcheck source=scripts/lib.sh
. "$(dirname "$0")/lib.sh"
need podman

[ -f "$OUT/image-ref" ] || die "no image built yet; run 'make image' first"
payload=$(cat "$OUT/image-ref")
bid=$(cat "$OUT/build-id")
installer="$IMAGE_NAME-installer:$MADOS_VERSION"
label=$(printf '%s-%s' "$MADOS_PRODUCT_ID" "$MADOS_VERSION" | tr 'a-z.' 'A-Z_' | cut -c1-32)
work="$OUT/build/iso"
rm -rf "$work"
mkdir -p "$work/output"

announce_sudo
log "building installer environment $installer"
$PODMAN build \
    --file "$ROOT/image/installer/Containerfile" \
    --build-arg "INSTALLER_BASE_IMAGE=$INSTALLER_BASE_IMAGE" \
    --build-arg "FEDORA_VERSION=$FEDORA_VERSION" \
    --build-arg "ISO_LABEL=$label" \
    --build-arg "PRODUCT_NAME=$MADOS_PRODUCT_NAME" \
    --build-arg "PAYLOAD_REF=$payload" \
    --build-arg "TARGET_REF=$UPDATE_IMAGE_REF:$MADOS_VERSION" \
    --tag "$installer" \
    "$ROOT/image/installer"

log "building ISO (bootc-generic-iso) with payload $payload"
$PODMAN run --rm --privileged --pull=newer \
    --security-opt label=type:unconfined_t \
    -v "$work/output:/output" \
    -v /var/lib/containers/storage:/var/lib/containers/storage \
    "$IMAGE_BUILDER" \
    build --output-dir /output \
    --bootc-ref "$installer" \
    --bootc-installer-payload-ref "$payload" \
    --bootc-default-fs "$ROOTFS" \
    bootc-generic-iso

iso=$(find "$work/output" -name '*.iso' | head -n 1)
[ -n "$iso" ] || die "image-builder produced no ISO (see output above)"
final="$OUT/$MADOS_PRODUCT_ID-$MADOS_VERSION-$bid-installer.iso"
$SUDO mv "$iso" "$final"
[ -z "$SUDO" ] || $SUDO chown "$(id -u):$(id -g)" "$final"
sha256_file "$final"
log "installer ISO: $final (experimental; boot it with: make vm-iso)"
