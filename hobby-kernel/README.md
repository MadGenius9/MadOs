# hobby-kernel

A small 32-bit x86 kernel written from scratch in freestanding C and NASM,
booted by GRUB via Multiboot2 (or directly by QEMU via Multiboot1). It is an educational project and is **not
part of the MadOS product**: the MadOS image is built from Fedora Kinoite
(see the top-level `README.md`). Nothing in `image/` or `system/` uses this
directory.

## What it does

At boot it brings up, in order (each line is printed with `[ OK ]`):

| Phase | Component | Files |
|---|---|---|
| 2 | Multiboot2 header (plus a Multiboot1 header for `qemu -kernel`), 16 KiB stack, entry point; kernel linked at 1 MiB | `src/boot.asm`, `linker.ld`, `src/multiboot1.c` |
| 3 | `kernel_main`, Makefile, GRUB ISO | `src/kernel.c`, `Makefile`, `grub.cfg` |
| 4 | VGA text driver (0xB8000, colours, scrolling, hardware cursor), `kprintf`, panic; output mirrored to COM1 | `src/vga.c`, `src/kprintf.c`, `src/serial.c` |
| 5 | GDT (flat ring 0/3 segments); IDT with all 32 CPU exceptions and a register dump | `src/gdt.c`, `src/gdt.asm`, `src/idt.c`, `src/isr.asm` |
| 6 | 8259 PIC remapped to vectors 32–47 (spurious IRQ aware); PIT at 100 Hz; PS/2 keyboard (scancode set 1, US layout, Shift/Caps/Ctrl) | `src/pic.c`, `src/pit.c`, `src/keyboard.c` |
| 7 | Bitmap physical memory manager from the Multiboot2 memory map; paging (identity map of 0–8 MiB with page 0 unmapped to catch NULL, recursive page directory, page-fault handler); kernel heap at 0xD0000000 with `kmalloc`/`kcalloc`/`kfree` that grows by mapping PMM frames | `src/pmm.c`, `src/paging.c`, `src/heap.c` |
| 8 | Preemptive multitasking: per-task kernel stacks, context switch by returning a different interrupt frame, round-robin scheduler with 50 ms time slices, sleep, blocking keyboard reads, idle task, kernel threads | `src/task.c`, `src/isr.asm`, `src/sync.h` |
| 9 | User mode: TSS, ring-3 processes with their own page directory (kernel half shared), `int 0x80` system calls with checked user pointers, faults in ring 3 kill only the process, built-in NASM programs | `src/syscall.c`, `src/gdt.c`, `src/paging.c`, `user/`, `src/programs.*` |

Then it starts a tiny shell: `help`, `clear`, `echo`, `mem`, `heap`,
`memtest`, `uptime`, `ps`, `run NAME [&]`, `kill PID`, `spawn ticker`,
`crash null`, `crash div`, `reboot`. Ctrl+C stops a foreground program.

## User programs and system calls

`run` with no argument lists the programs in `user/`, which are assembled
into flat binaries and embedded in the kernel:

| Program | What it shows |
|---|---|
| `hello` | runs in ring 3, calls `getpid` and `write`, exits with code 0 |
| `counter` | prints five lines with `sleep` in between; run two with `&` to see them interleave |
| `spin` | an endless loop with no system calls: the timer still preempts it |
| `badptr` | `write()` with a kernel pointer is refused (`-EFAULT`); writing kernel memory directly gets the process killed |
| `priv` | `cli` in ring 3 raises a general protection fault; the process is killed |

System calls use `int 0x80` with the number in EAX and arguments in EBX,
ECX, EDX (see `src/syscall.h` and `user/syscalls.inc`): `exit(code)`,
`write(buf, len)`, `getpid()`, `sleep(ms)`, `yield()`.

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
make run-kernel # boot build/kernel.elf directly, no GRUB/ISO needed
make smoke      # headless boot test that types into the shell (see below)
```

`make smoke` (`tools/smoke_test.py`) boots QEMU without a window, waits for
`BOOT_OK` on the serial port, then types commands as PS/2 key presses via
the QEMU monitor and checks every answer, over three boots:

1. `echo` with Shift, an unknown command, `mem`, `memtest`, `heap`,
   `uptime` (compared with the wall clock), `crash null` (page-fault handler).
2. `crash div` (CPU exception handler).
3. Processes: `ps`, a kernel thread ticking while the shell answers, every
   user program above, two background processes at once, a ring-3 busy loop
   that is preempted, listed, killed with `kill` and with Ctrl+C, and leak
   checks (free frames and heap use return to their earlier values).

It saves the VGA screen as `build/screen.png`.
`make smoke-iso` runs the same test against the GRUB ISO.

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
- **Boots in QEMU 9.2** (`qemu-system-i386 -kernel`, Multiboot1 path) and
  `make smoke` passes all 29 steps.
- **Not yet tested: the GRUB ISO path** (`make iso`, `make run`,
  `make smoke-iso`), because the build environment had no GRUB or xorriso.
  It differs from the tested path only in the loader and in reading the
  Multiboot2 info directly instead of converting Multiboot1 info. Also
  untested on real hardware.

## Memory layout

| Virtual address | What |
|---|---|
| 0x00000000 | unmapped (NULL guard page) |
| 0x00001000–0x007FFFFF | identity map: BIOS area, VGA at 0xB8000, kernel from 0x00100000 (kernel only) |
| 0x40000000–0xBFFFFFFF | user process: program at 0x40000000, 16 KiB stack ending at 0xC0000000 |
| 0xD0000000–0xDFFFFFFF | kernel heap, mapped on demand (kernel only) |
| 0xFFBFF000 | scratch page for editing a page directory that is not loaded |
| 0xFFC00000–0xFFFFFFFF | recursive mapping: page tables, page directory at 0xFFFFF000 |

Every process has its own page directory. The kernel entries are copied
from the kernel's directory and point at the same page tables (the heap's
are created at boot), so kernel mappings are identical everywhere.

The Multiboot2 information structure is read before paging is enabled,
because GRUB may place it above 8 MiB.
