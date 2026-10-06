# Hardware support

MadOS 0.1 targets **x86_64 machines with UEFI firmware**. All testing so far
is in virtual machines.

| Target | Status |
|---|---|
| QEMU/KVM, q35 + OVMF (UEFI), virtio devices | primary development target; smoke-tested in CI |
| QEMU without KVM (TCG) | works for the harness self-test; a full desktop boot is very slow |
| Other hypervisors (VirtualBox, VMware, Hyper-V) | untested |
| Physical laptops | **untested — do not install yet.** Physical hardware testing is milestone M8 and will be done deliberately, on spare hardware, with a separate plan |
| BIOS/CSM-only machines | not supported |
| ARM64 | not supported in 0.1 |

Hardware enablement (kernel, firmware, Mesa, Wi-Fi, Bluetooth, audio) comes
unchanged from Fedora 44, so hardware Fedora Kinoite supports is the expected
baseline once physical testing starts.

## Platform adjustments

- `mcelog.service` (Fedora) supports Intel CPUs only and fails on AMD
  family 17h+, leaving the system "degraded". MadOS adds a drop-in
  (`ExecCondition=/usr/libexec/mados/is-intel-cpu`) so it is skipped on
  non-Intel CPUs; AMD machine-check reporting uses the kernel's EDAC
  drivers. Found by the VM smoke test on an AMD CI host.

## Secure Boot

Fedora's shim and GRUB are signed by Microsoft's third-party CA, so a MadOS
image *should* boot with Secure Boot enabled using Fedora's stock kernel. This
is untested. MadOS does not sign anything itself yet; custom kernel modules
would require MOK enrollment. Secure Boot compatibility is a long-term
requirement (see [security.md](../security.md)).

## Reporting hardware results (from M8)

Include: model, firmware version, `madosctl about --json`,
`journalctl -b -p warning`, and what did/didn't work (graphics, Wi-Fi,
Bluetooth, audio, suspend, backlight, touchpad).
