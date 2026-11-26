# hobby-kernel build.
#
#   make              build build/kernel.elf and build/initrd.tar (i686-elf cross-compiler)
#   make iso          build build/hobby-kernel.iso (needs grub-mkrescue, xorriso, mtools)
#   make run          boot the ISO in QEMU (serial output in this terminal)
#   make run-curses   same, VGA text shown in the terminal (no GUI needed)
#   make run-kernel   boot the ELF directly with QEMU's Multiboot1 loader (no GRUB)
#   make smoke        boot in QEMU headless and drive the shell (tools/smoke_test.py)
#   make test         host unit tests (allocators, tar filesystem) + Multiboot header check
#   make TOOLCHAIN=host   use the system gcc -m32 instead of i686-elf-gcc

TOOLCHAIN ?= cross

ifeq ($(TOOLCHAIN),cross)
CC := i686-elf-gcc
LIBGCC := -lgcc
else ifeq ($(TOOLCHAIN),host)
CC := gcc -m32
LIBGCC :=
else
$(error TOOLCHAIN must be 'cross' or 'host')
endif

AS := nasm
HOSTCC ?= cc
QEMU ?= qemu-system-i386
PYTHON ?= python3

BUILD := build
ISO := $(BUILD)/hobby-kernel.iso
KERNEL := $(BUILD)/kernel.elf
INITRD := $(BUILD)/initrd.tar

# -mgeneral-regs-only: no x87/SSE code, since the kernel never sets up the FPU.
# The -fno-* flags turn off hosted-distro defaults (PIE, stack protector, CET)
# that need runtime support a bare-metal kernel does not have.
CFLAGS := -std=gnu11 -ffreestanding -O2 -g -Wall -Wextra -Werror \
          -march=i686 -mgeneral-regs-only \
          -fno-pie -fno-stack-protector -fcf-protection=none \
          -fno-asynchronous-unwind-tables -fno-tree-loop-distribute-patterns
ASFLAGS := -f elf32 -g -F dwarf
LDFLAGS := -T linker.ld -ffreestanding -nostdlib -no-pie -Wl,--build-id=none -Wl,-z,noexecstack

C_SOURCES := $(wildcard src/*.c)
ASM_SOURCES := $(wildcard src/*.asm)
OBJECTS := $(patsubst src/%.asm,$(BUILD)/%.asm.o,$(ASM_SOURCES)) \
           $(patsubst src/%.c,$(BUILD)/%.o,$(C_SOURCES))
DEPS := $(patsubst src/%.c,$(BUILD)/%.d,$(C_SOURCES))

# User programs: ELF executables in /bin of the initial ramdisk.
USER_CFLAGS := $(CFLAGS) -Iuser/lib
USER_LDFLAGS := -T user/user.ld -ffreestanding -nostdlib -no-pie -s -Wl,--build-id=none -Wl,-z,noexecstack
USER_LIB := $(BUILD)/user/lib/crt0.o $(BUILD)/user/lib/ulib.o
USER_C_PROGRAMS := $(patsubst user/%.c,%,$(wildcard user/*.c))
USER_ASM_PROGRAMS := $(patsubst user/%.asm,%,$(wildcard user/*.asm))
USER_BINS := $(addprefix $(BUILD)/initrd/bin/,$(USER_C_PROGRAMS) $(USER_ASM_PROGRAMS))
INITRD_FILES := $(shell find initrd -type f 2>/dev/null)

QEMU_FLAGS := -m 128M -serial stdio -no-reboot -no-shutdown

.PHONY: all iso run run-curses run-kernel smoke smoke-iso test check clean

all: $(KERNEL) $(INITRD)

$(BUILD):
	mkdir -p $@

$(BUILD)/%.o: src/%.c | $(BUILD)
	$(CC) $(CFLAGS) -MMD -MP -c $< -o $@

$(BUILD)/%.asm.o: src/%.asm | $(BUILD)
	$(AS) $(ASFLAGS) $< -o $@

$(KERNEL): $(OBJECTS) linker.ld
	$(CC) $(LDFLAGS) -o $@ $(OBJECTS) $(LIBGCC)

$(BUILD)/user/lib/crt0.o: user/lib/crt0.asm
	@mkdir -p $(dir $@)
	$(AS) $(ASFLAGS) $< -o $@

$(BUILD)/user/lib/ulib.o: user/lib/ulib.c user/lib/ulib.h
	@mkdir -p $(dir $@)
	$(CC) $(USER_CFLAGS) -c $< -o $@

$(BUILD)/user/%.o: user/%.c user/lib/ulib.h
	@mkdir -p $(dir $@)
	$(CC) $(USER_CFLAGS) -c $< -o $@

$(BUILD)/user/%.asm.o: user/%.asm user/syscalls.inc
	@mkdir -p $(dir $@)
	$(AS) $(ASFLAGS) -Iuser/ $< -o $@

$(addprefix $(BUILD)/initrd/bin/,$(USER_C_PROGRAMS)): $(BUILD)/initrd/bin/%: $(BUILD)/user/%.o $(USER_LIB) user/user.ld
	@mkdir -p $(dir $@)
	$(CC) $(USER_LDFLAGS) -o $@ $(USER_LIB) $< $(LIBGCC)

$(addprefix $(BUILD)/initrd/bin/,$(USER_ASM_PROGRAMS)): $(BUILD)/initrd/bin/%: $(BUILD)/user/%.asm.o user/user.ld
	@mkdir -p $(dir $@)
	$(CC) $(USER_LDFLAGS) -o $@ $<

$(INITRD): $(USER_BINS) $(INITRD_FILES) tools/mkinitrd.py
	$(PYTHON) tools/mkinitrd.py $@ initrd $(BUILD)/initrd

iso: $(ISO)

$(ISO): $(KERNEL) $(INITRD) grub.cfg
	rm -rf $(BUILD)/isodir
	mkdir -p $(BUILD)/isodir/boot/grub
	cp $(KERNEL) $(BUILD)/isodir/boot/kernel.elf
	cp $(INITRD) $(BUILD)/isodir/boot/initrd.tar
	cp grub.cfg $(BUILD)/isodir/boot/grub/grub.cfg
	grub-mkrescue -o $@ $(BUILD)/isodir

run: $(ISO)
	$(QEMU) -cdrom $(ISO) $(QEMU_FLAGS)

run-curses: $(ISO)
	$(QEMU) -cdrom $(ISO) -m 128M -display curses -no-reboot -no-shutdown

run-kernel: $(KERNEL) $(INITRD)
	$(QEMU) -kernel $(KERNEL) -initrd $(INITRD) $(QEMU_FLAGS)

smoke: $(KERNEL) $(INITRD)
	$(PYTHON) tools/smoke_test.py --qemu $(QEMU) --kernel $(KERNEL) --initrd $(INITRD) --screenshot $(BUILD)/screen.png

smoke-iso: $(ISO)
	$(PYTHON) tools/smoke_test.py --qemu $(QEMU) --iso $(ISO) --screenshot $(BUILD)/screen-iso.png

check: $(KERNEL)
	$(PYTHON) tools/check_multiboot2.py $(KERNEL)

TEST_CFLAGS := -std=gnu11 -DUNIT_TEST -O1 -g -Wall -Wextra -Werror -iquote src -fPIE -pie -fsanitize=address,undefined

$(BUILD)/heap_test: tests/heap_test.c src/heap.c src/heap.h | $(BUILD)
	$(HOSTCC) $(TEST_CFLAGS) $< -o $@

$(BUILD)/pmm_test: tests/pmm_test.c src/pmm.c src/pmm.h src/multiboot2.h | $(BUILD)
	$(HOSTCC) $(TEST_CFLAGS) $< -o $@

$(BUILD)/fs_test: tests/fs_test.c src/fs.c src/fs.h | $(BUILD)
	$(HOSTCC) $(TEST_CFLAGS) $< -o $@

$(BUILD)/elf_test: tests/elf_test.c src/elf.c src/elf.h | $(BUILD)
	$(HOSTCC) $(TEST_CFLAGS) $< -o $@

test: $(BUILD)/heap_test $(BUILD)/pmm_test $(BUILD)/fs_test $(BUILD)/elf_test $(INITRD) check
	$(BUILD)/heap_test
	$(BUILD)/pmm_test
	$(BUILD)/fs_test $(INITRD)
	$(BUILD)/elf_test $(BUILD)/initrd/bin/cat

clean:
	rm -rf $(BUILD)

-include $(DEPS)
