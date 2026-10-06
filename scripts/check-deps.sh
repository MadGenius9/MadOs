#!/bin/sh
# make setup: checks development prerequisites and explains what is missing.
# Installs nothing.
set -u
ROOT=$(cd "$(dirname "$0")/.." && pwd)
missing=0
warn=0

ok()   { printf '  [ok]      %s\n' "$*"; }
bad()  { printf '  [missing] %s\n' "$*"; missing=$((missing + 1)); }
note() { printf '  [warn]    %s\n' "$*"; warn=$((warn + 1)); }
have() { command -v "$1" >/dev/null 2>&1; }

if have dnf; then pm=dnf; elif have apt-get; then pm=apt; elif have pacman; then pm=pacman; else pm=unknown; fi
hint() {
    case $pm in
        dnf) printf '            sudo dnf install %s\n' "$1" ;;
        apt) printf '            sudo apt-get install %s\n' "$2" ;;
        pacman) printf '            sudo pacman -S %s\n' "$3" ;;
        *) printf '            install: %s\n' "$1" ;;
    esac
}

echo "Components (make build / make test):"
if have cargo && have rustc; then ok "Rust $(rustc --version | cut -d' ' -f2)"; else bad "Rust toolchain"; printf '            https://rustup.rs\n'; fi
if have pkg-config && pkg-config --exists gtk4 2>/dev/null; then ok "GTK 4 development files $(pkg-config --modversion gtk4)"; else bad "GTK 4 development files"; hint gtk4-devel libgtk-4-dev gtk4; fi
if have pkg-config && pkg-config --exists libpulse 2>/dev/null; then ok "libpulse development files $(pkg-config --modversion libpulse)"; else bad "libpulse development files (sound settings)"; hint pulseaudio-libs-devel libpulse-dev libpulse; fi
if have python3 && python3 -c 'import sys; sys.exit(sys.version_info < (3, 11))'; then ok "Python $(python3 -c 'import platform; print(platform.python_version())')"; else bad "Python >= 3.11 (tomllib)"; hint python3 python3 python; fi
if have dbus-daemon; then ok "dbus-daemon (D-Bus integration tests)"; else note "dbus-daemon missing: D-Bus integration tests will be skipped"; hint dbus-daemon dbus dbus; fi
if have pulseaudio; then ok "pulseaudio (audio integration test server)"; else note "pulseaudio missing: the audio integration test will be skipped"; hint pulseaudio pulseaudio pulseaudio; fi
if have shellcheck; then ok "shellcheck"; else note "shellcheck missing: shell lint skipped"; hint ShellCheck shellcheck shellcheck; fi
if have rsvg-convert; then ok "rsvg-convert"; else note "rsvg-convert missing: wallpaper staged as SVG locally"; hint librsvg2-tools librsvg2-bin librsvg; fi

echo "Image (make image / make disk / make iso):"
if have podman; then ok "podman $(podman --version | awk '{print $3}')"; else bad "podman"; hint podman podman podman; fi
if have openssl; then ok "openssl (dev password hashing)"; else bad "openssl"; hint openssl openssl openssl; fi
avail_kb=$(df -Pk "$ROOT" | awk 'NR==2 {print $4}')
if [ "${avail_kb:-0}" -ge 52428800 ]; then ok "disk space $((avail_kb / 1048576)) GiB free"; else note "only $((avail_kb / 1048576)) GiB free; image builds need ~50 GiB (repo dir and /var/lib/containers)"; fi
for h in quay.io ghcr.io; do
    if have curl && curl -s -o /dev/null --max-time 10 "https://$h/v2/"; then ok "network access to $h"; else note "cannot reach $h (needed to pull base/builder images)"; fi
done

echo "Virtual machines (make vm / make smoke):"
if have qemu-system-x86_64; then ok "QEMU $(qemu-system-x86_64 --version | head -n1 | awk '{print $4}')"; else bad "qemu-system-x86_64"; hint qemu-kvm qemu-system-x86 qemu-full; fi
if have qemu-img; then ok "qemu-img"; else bad "qemu-img"; hint qemu-img qemu-utils qemu-img; fi
if python3 -c "import sys; sys.path.insert(0, '$ROOT/scripts'); import vm; sys.exit(vm.find_ovmf() is None)"; then ok "UEFI firmware (OVMF)"; else bad "UEFI firmware (OVMF)"; hint edk2-ovmf ovmf edk2-ovmf; fi
if [ -e /dev/kvm ] && [ -r /dev/kvm ] && [ -w /dev/kvm ]; then ok "KVM acceleration"; elif [ -e /dev/kvm ]; then note "/dev/kvm exists but is not accessible: add yourself to the 'kvm' group"; else note "no /dev/kvm: VMs will use slow software emulation"; fi

echo
if [ "$missing" -gt 0 ]; then
    echo "setup: $missing required item(s) missing, $warn warning(s). Nothing was installed."
    exit 1
fi
echo "setup: all required tools present ($warn warning(s))."
