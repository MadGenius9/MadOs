#!/usr/bin/env python3
"""Check that an ELF kernel is loadable by a Multiboot2 bootloader.

    python3 tools/check_multiboot2.py build/kernel.elf

Verifies what GRUB's `multiboot2` command needs: a 32-bit x86 ELF, a valid
Multiboot2 header (magic, architecture, length, checksum, tags ending in an
end tag) that is 8-byte aligned within the first 32 KiB of the file, and an
entry point inside an executable loadable segment at or above 1 MiB.
Also checks the optional Multiboot1 header (used by `qemu-system-i386
-kernel`): 4-byte aligned in the first 8 KiB, with a valid checksum.
"""
from __future__ import annotations

import struct
import sys

MB2_MAGIC = 0xE85250D6
MB1_MAGIC = 0x1BADB002
SEARCH_LIMIT = 32768
MB1_SEARCH_LIMIT = 8192
PT_LOAD = 1
PF_X = 1
EM_386 = 3


def fail(msg: str) -> None:
    print(f"FAIL: {msg}")
    sys.exit(1)


def check(path: str) -> None:
    data = open(path, "rb").read()

    if data[:4] != b"\x7fELF":
        fail("not an ELF file")
    if data[4] != 1:
        fail("not a 32-bit ELF (EI_CLASS != ELFCLASS32)")
    if data[5] != 1:
        fail("not little-endian")
    (e_type, e_machine, _ver, e_entry, e_phoff, _shoff, _flags, _ehsize, e_phentsize, e_phnum) = struct.unpack_from(
        "<HHIIIIIHHH", data, 16
    )
    if e_type != 2:
        fail(f"ELF type {e_type} is not ET_EXEC")
    if e_machine != EM_386:
        fail(f"ELF machine {e_machine} is not EM_386")

    entry_ok = False
    for i in range(e_phnum):
        p_type, _off, p_vaddr, p_paddr, _filesz, p_memsz, p_flags, _align = struct.unpack_from(
            "<IIIIIIII", data, e_phoff + i * e_phentsize
        )
        if p_type != PT_LOAD:
            continue
        if p_paddr < 0x100000:
            fail(f"loadable segment at 0x{p_paddr:08x} is below 1 MiB")
        if p_flags & PF_X and p_vaddr <= e_entry < p_vaddr + p_memsz:
            entry_ok = True
    if not entry_ok:
        fail(f"entry point 0x{e_entry:08x} is not in an executable PT_LOAD segment")

    for off in range(0, min(len(data), MB1_SEARCH_LIMIT) - 12 + 1, 4):
        magic, flags, checksum = struct.unpack_from("<III", data, off)
        if magic != MB1_MAGIC:
            continue
        if (magic + flags + checksum) & 0xFFFFFFFF:
            fail(f"Multiboot1 header at offset 0x{off:x} has a bad checksum")
        if flags & 0x10000:
            fail("Multiboot1 header asks for a.out address fields; this kernel relies on ELF loading")
        print(f"OK: Multiboot1 header at file offset 0x{off:x} (flags 0x{flags:x})")
        break

    for off in range(0, min(len(data), SEARCH_LIMIT) - 16 + 1, 8):
        magic, arch, length, checksum = struct.unpack_from("<IIII", data, off)
        if magic != MB2_MAGIC:
            continue
        if (magic + arch + length + checksum) & 0xFFFFFFFF:
            fail(f"header at offset 0x{off:x} has a bad checksum")
        if arch != 0:
            fail(f"header architecture {arch} is not 0 (i386 protected mode)")
        if off + length > len(data):
            fail("header runs past the end of the file")
        tag = off + 16
        tags = []
        while True:
            if tag + 8 > off + length:
                fail("header has no end tag")
            t_type, _t_flags, t_size = struct.unpack_from("<HHI", data, tag)
            tags.append(t_type)
            if t_type == 0:
                if t_size != 8:
                    fail("end tag size is not 8")
                break
            if t_size < 8:
                fail(f"tag {t_type} has size {t_size}")
            tag += (t_size + 7) & ~7
        if tag + 8 != off + length:
            fail(f"header length {length} does not match its tags")
        print(
            f"OK: Multiboot2 header at file offset 0x{off:x} (length {length}, tags {tags}); "
            f"entry 0x{e_entry:08x}"
        )
        return
    fail("no Multiboot2 header in the first 32 KiB")


if __name__ == "__main__":
    if len(sys.argv) != 2:
        print(__doc__)
        sys.exit(2)
    check(sys.argv[1])
