# hobby-kernel

A small 32-bit x86 kernel written from scratch in freestanding C and NASM,
booted by GRUB via Multiboot2. It is an educational project and is **not
part of the MadOS product**: the MadOS image is built from Fedora Kinoite
(see the top-level `README.md`). Nothing in `image/` or `system/` uses this
directory.

## What it does

At boot it brings up, in order (each line is printed with `[ OK ]`):

| Phase | Component | Files |
|---|---|---|
| 2 | Multiboot2 header, 16 KiB stack, entry point; kernel linked at 1 MiB | `src/boot.asm`, `linker.ld` |
| 3 | `kernel_main`, Makefile, GRUB ISO | `src/kernel.c`, `Makefile`, `grub.cfg` |
| 4 | VGA text driver (0xB8000, colours, scrolling, hardware cursor), `kprintf`, panic; output mirrored to COM1 | `src/vga.c`, `src/kprintf.c`, `src/serial.c` |
| 5 | GDT (flat ring 0/3 segments); IDT with all 32 CPU exceptions and a register dump | `src/gdt.c`, `src/gdt.asm`, `src/idt.c`, `src/isr.asm` |
| 6 | 8259 PIC remapped to vectors 32–47 (spurious IRQ aware); PIT at 100 Hz; PS/2 keyboard (scancode set 1, US layout, Shift/Caps/Ctrl) | `src/pic.c`, `src/pit.c`, `src/keyboard.c` |
| 7 | Bitmap physical memory manager from the Multiboot2 memory map; paging (identity map of 0–8 MiB with page 0 unmapped to catch NULL, recursive page directory, page-fault handler); kernel heap at 0xD0000000 with `kmalloc`/`kcalloc`/`kfree` that grows by mapping PMM frames | `src/pmm.c`, `src/paging.c`, `src/heap.c` |

Then it starts a tiny shell: `help`, `clear`, `echo`, `mem`, `heap`,
`memtest`, `uptime`, `crash null`, `crash div`, `reboot`.

## Build and run (Ubuntu / WSL2)

Prerequisites: the `i686-elf` cross-compiler in `PATH` (Phase 1), plus

```sh
sudo apt install nasm grub-pc-bin grub-common xorriso mtools qemu-system-x86
```

```sh
make            # build/kernel.elf
make test       # host unit tests for the PMM and heap + Multiboot2 header check
make iso        # build/hobby-kernel.iso
make run        # boot in QEMU; serial log appears in this terminal
make run-curses # VGA text in the terminal (WSL2 on Windows 10, SSH, ...)
```

`make TOOLCHAIN=host` builds with the system `gcc -m32` instead of
`i686-elf-gcc`, which is useful when no cross-compiler is installed.

The ISO boots through GRUB's BIOS (i386-pc) path, which is what QEMU uses by
default. VGA text mode does not exist under UEFI, so a UEFI-only boot would
need a framebuffer console instead.

A successful boot ends with `BOOT_OK` on the serial port, followed by the
shell prompt.

## Status

- Compiles warning-free with `-Wall -Wextra -Werror` (cross and host
  toolchains). `make test` passes: the PMM and heap run on the host under
  AddressSanitizer/UBSan, and the ELF's Multiboot2 header is checked.
- **Not yet booted in QEMU or on hardware.** The environment it was written
  in had no QEMU, GRUB or xorriso. Run `make run` and report what you see.

## Memory layout

| Virtual address | What |
|---|---|
| 0x00000000 | unmapped (NULL guard page) |
| 0x00001000–0x007FFFFF | identity map: BIOS area, VGA at 0xB8000, kernel from 0x00100000 |
| 0xD0000000–0xDFFFFFFF | kernel heap, mapped on demand |
| 0xFFC00000–0xFFFFFFFF | recursive mapping: page tables, page directory at 0xFFFFF000 |

The Multiboot2 information structure is read before paging is enabled,
because GRUB may place it above 8 MiB.
