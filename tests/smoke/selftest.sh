#!/bin/sh
# Self-test of the VM smoke harness (tests/smoke/vm_smoke.py) without a
# MadOS image: boots a tiny guest (host-provided Linux kernel + busybox
# initramfs) that prints the same serial markers as mados-boot-report, then
# powers itself off. Verifies QEMU/OVMF invocation, serial capture, marker
# parsing, QMP screenshots and shutdown detection.
#
# Kernel: $MADOS_SELFTEST_KERNEL, else /boot/vmlinuz-$(uname -r) if readable,
# else (Debian/Ubuntu only) the linux-image-generic package is DOWNLOADED (not
# installed) into out/selftest/.
set -eu
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
WORK="$ROOT/out/selftest"
mkdir -p "$WORK"

kernel=${MADOS_SELFTEST_KERNEL:-}
if [ -z "$kernel" ] && [ -r "/boot/vmlinuz-$(uname -r)" ]; then
    kernel="/boot/vmlinuz-$(uname -r)"
fi
if [ -z "$kernel" ]; then
    found=$(find "$WORK/kpkg" -name 'vmlinuz-*' 2>/dev/null | head -n 1 || true)
    if [ -n "$found" ]; then
        kernel=$found
    elif command -v apt-get >/dev/null 2>&1; then
        echo "selftest: downloading a kernel package (not installing it)"
        dep=$(apt-cache depends linux-image-generic | awk '/Depends: linux-image-[0-9]/ {print $2; exit}')
        (cd "$WORK" && apt-get download "$dep" >/dev/null)
        dpkg-deb -x "$WORK/${dep}"_*.deb "$WORK/kpkg"
        kernel=$(find "$WORK/kpkg" -name 'vmlinuz-*' | head -n 1)
    else
        echo "selftest: no kernel available; set MADOS_SELFTEST_KERNEL" >&2
        exit 2
    fi
fi

busybox=$(command -v busybox || true)
if [ -z "$busybox" ] || ! file "$busybox" 2>/dev/null | grep -q 'statically linked'; then
    for b in /bin/busybox /usr/bin/busybox /usr/lib/busybox/busybox-static; do
        if [ -x "$b" ] && file "$b" | grep -q 'statically linked'; then busybox=$b; break; fi
    done
fi
[ -n "$busybox" ] || { echo "selftest: a static busybox is required (busybox-static)" >&2; exit 2; }

rm -rf "$WORK/initramfs"
mkdir -p "$WORK/initramfs/bin" "$WORK/initramfs/proc" "$WORK/initramfs/sys" "$WORK/initramfs/dev"
cp "$busybox" "$WORK/initramfs/bin/busybox"
cat > "$WORK/initramfs/init" <<'INIT'
#!/bin/busybox sh
/bin/busybox --install -s /bin
mount -t proc proc /proc
mount -t sysfs sys /sys
mount -t devtmpfs dev /dev 2>/dev/null
out=/dev/ttyS0
echo "MADOS_BOOT_OK version=selftest build=selftest kernel=$(uname -r) selinux=enforcing state=running failed=none" > $out
echo "MADOS_SESSION_OK type=selftest class=user desktop=selftest user=mados" > $out
echo "MADOS_APPS terminal=ok files=ok browser=running settings=ok audio=unknown daemon=ok bootc=ok assistant=ok first_run=running kde_welcome=absent" > $out
sleep 25
echo "MADOS_SHUTDOWN" > $out
poweroff -f
INIT
chmod 0755 "$WORK/initramfs/init"
(cd "$WORK/initramfs" && find . | cpio -o -H newc --quiet | gzip -1) > "$WORK/initramfs.img"

echo "selftest: kernel $kernel"
exec python3 "$ROOT/tests/smoke/vm_smoke.py" \
    --kernel "$kernel" --initrd "$WORK/initramfs.img" \
    --append "console=ttyS0 panic=-1 quiet" \
    --workdir "$WORK/run" --memory 512 --cpus 1 \
    --timeout 600 --session-timeout 30 --apps-timeout 30 --agent-timeout 3 --no-reboot --settle 1 \
    --shutdown-timeout 120
