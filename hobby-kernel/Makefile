# hobby-kernel build.
#
#   make              build build/kernel.elf with the i686-elf cross-compiler
#   make iso          build build/hobby-kernel.iso (needs grub-mkrescue, xorriso, mtools)
#   make run          boot the ISO in QEMU (serial output in this terminal)
#   make run-curses   same, VGA text shown in the terminal (no GUI needed)
#   make test         host unit tests for the allocators + Multiboot2 header check
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

QEMU_FLAGS := -m 128M -serial stdio -no-reboot -no-shutdown

.PHONY: all iso run run-curses test check clean

all: $(KERNEL)

$(BUILD):
	mkdir -p $@

$(BUILD)/%.o: src/%.c | $(BUILD)
	$(CC) $(CFLAGS) -MMD -MP -c $< -o $@

$(BUILD)/%.asm.o: src/%.asm | $(BUILD)
	$(AS) $(ASFLAGS) $< -o $@

$(KERNEL): $(OBJECTS) linker.ld
	$(CC) $(LDFLAGS) -o $@ $(OBJECTS) $(LIBGCC)

iso: $(ISO)

$(ISO): $(KERNEL) grub.cfg
	rm -rf $(BUILD)/isodir
	mkdir -p $(BUILD)/isodir/boot/grub
	cp $(KERNEL) $(BUILD)/isodir/boot/kernel.elf
	cp grub.cfg $(BUILD)/isodir/boot/grub/grub.cfg
	grub-mkrescue -o $@ $(BUILD)/isodir

run: $(ISO)
	$(QEMU) -cdrom $(ISO) $(QEMU_FLAGS)

run-curses: $(ISO)
	$(QEMU) -cdrom $(ISO) -m 128M -display curses -no-reboot -no-shutdown

check: $(KERNEL)
	$(PYTHON) tools/check_multiboot2.py $(KERNEL)

TEST_CFLAGS := -std=gnu11 -O1 -g -Wall -Wextra -Werror -iquote src -fPIE -pie -fsanitize=address,undefined

$(BUILD)/heap_test: tests/heap_test.c src/heap.c src/heap.h | $(BUILD)
	$(HOSTCC) $(TEST_CFLAGS) $< -o $@

$(BUILD)/pmm_test: tests/pmm_test.c src/pmm.c src/pmm.h src/multiboot2.h | $(BUILD)
	$(HOSTCC) $(TEST_CFLAGS) $< -o $@

test: $(BUILD)/heap_test $(BUILD)/pmm_test check
	$(BUILD)/heap_test
	$(BUILD)/pmm_test

clean:
	rm -rf $(BUILD)

-include $(DEPS)
