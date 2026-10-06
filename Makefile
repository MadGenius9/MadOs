# MadOS developer entry points. See docs/development/building.md.
#
#   make setup         check prerequisites (installs nothing)
#   make build         build MadOS components (Rust workspace)
#   make test          unit + integration tests, lint, config validation,
#                      VM-harness self-test
#   make image         build the bootable container image   (root podman)
#   make disk          image -> bootable UEFI qcow2         (root podman)
#   make iso           image -> installer ISO, EXPERIMENTAL (root podman)
#   make vm            boot the newest disk image in QEMU
#   make vm-iso        boot the newest installer ISO in QEMU (scratch disk)
#   make smoke         automated boot/session/network/reboot/shutdown test
#   make iso-test      unattended install from the ISO, then smoke-test it
#   make clean         remove build outputs

SHELL := /bin/sh
CARGO ?= cargo
PYTHON ?= python3
VERSION := $(shell $(PYTHON) scripts/product.py version)
STAGING := out/staging

.PHONY: all setup build build-release stage test test-unit test-lint test-config \
        test-harness image disk iso vm vm-iso smoke iso-test clean version help

all: build

help:
	@sed -n '1,/^$$/p' Makefile | sed 's/^# \{0,1\}//'

version:
	@echo $(VERSION)

setup:
	@sh scripts/check-deps.sh

build:
	$(CARGO) build --workspace --locked

build-release:
	$(CARGO) build --workspace --release --locked

# Stage the root-filesystem overlay exactly as the image build does.
stage: build-release
	rm -rf $(STAGING)
	$(PYTHON) scripts/stage-system.py --bin-dir target/release --destdir $(STAGING) \
		--build-id local --git-commit "$$(git rev-parse --short=12 HEAD 2>/dev/null || echo unknown)" \
		--base-image local

test: test-lint test-unit test-config test-harness

test-unit:
	$(CARGO) test --workspace --locked

test-lint:
	$(CARGO) fmt --all -- --check
	$(CARGO) clippy --workspace --all-targets --locked -- -D warnings

test-config: stage
	$(PYTHON) tests/config/validate.py --staging $(STAGING)

# Boots a tiny guest to prove the smoke-test harness works (no MadOS image needed).
test-harness:
	sh tests/smoke/selftest.sh

image:
	sh scripts/build-image.sh

disk:
	sh scripts/build-disk.sh

iso:
	sh scripts/build-iso.sh

vm:
	$(PYTHON) scripts/vm.py run

vm-iso:
	$(PYTHON) scripts/vm.py run --iso "$$(ls -t out/*.iso 2>/dev/null | head -n1)"

smoke:
	$(PYTHON) tests/smoke/vm_smoke.py --require-session --require-apps

iso-test:
	$(PYTHON) tests/smoke/iso_install.py

clean:
	$(CARGO) clean
	rm -rf out
