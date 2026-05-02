


# Architecture

Rhubarb consists of 2 major components:

- Bare metal operating system kernel (in [/kernel](../kernel), with a simple example program in [/example](../example)).
- UEFI Bootloader (in [/bootloader](../bootloader)).

The [/crates](../crates) directory contains various crates (e.g. [`boot-info`](../crates/boot-info)) used by one or more of the major components.

The [/drivers](../drivers) directory contains device drivers used by the kernel.

## Current State

✔️ Up To Date, ❌ Stale

- [/bootloader](../bootloader) ✔️
- [/crates](../crates)
  - [/abi](../crates/abi) ❌
  - [/abi-tests](../crates/abi-tests) ✔️
  - [/bit-utils](../crates/bit-utils) ✔️
  - [/boot-info](../crates/boot-info) ✔️
  - [/defer-mutex](../crates/defer-mutex) ✔️
  - [/elf](../crates/elf) ✔️
  - [/framebuffer](../crates/framebuffer) ✔️
  - [/input](../crates/input) ✔️
  - [/log](../crates/log) ✔️
  - [/math](../crates/math) ✔️
  - [/memory-types](../crates/memory-types) ✔️
  - [/pod](../crates/pod) ✔️
  - [/spin-mutex](../crates/spin-mutex) ✔️
  - [/time](../crates/time) ✔️
- [/drivers](../drivers)
  - [/pci](../drivers/pci) ✔️
  - [/pit](../drivers/pit) ✔️
  - [/virtio](../drivers/virtio) ✔️
  - [/x86-port](../drivers/x86-port) ✔️
- [/example](../example) ✔️
- [/kernel](../kernel) ✔️
