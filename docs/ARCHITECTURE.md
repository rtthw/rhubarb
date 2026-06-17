
# Architecture

Rhubarb consists of 3 major components:

- Bare metal operating system kernel (in [/kernel](../kernel).
- A shell implementation in [/shell](../shell)), with applications in [/shell/apps](../shell/apps).
- UEFI Bootloader (in [/bootloader](../bootloader)).

The [/crates](../crates) directory contains various crates (e.g. [`boot-info`](../crates/boot-info)) used by one or more of the major components.

The [/drivers](../drivers) directory contains device drivers used by both the kernel and userspace applications.

## Current State

✔️ Up To Date, ❌ Stale

- [/bootloader](../bootloader) ✔️
- [/crates](../crates)
  - [/bit-utils](../crates/bit-utils) ✔️
  - [/boot-info](../crates/boot-info) ✔️
  - [/defer-mutex](../crates/defer-mutex) ✔️
  - [/elf](../crates/elf) ✔️
  - [/framebuffer](../crates/framebuffer) ✔️
  - [/fs](../crates/fs) ✔️
  - [/heap](../crates/heap) ✔️
  - [/input](../crates/input) ✔️
  - [/io](../crates/io) ✔️
  - [/log](../crates/log) ✔️
  - [/math](../crates/math) ✔️
  - [/memory-types](../crates/memory-types) ✔️
  - [/panic](../crates/panic) ✔️
  - [/pod](../crates/pod) ✔️
  - [/process](../crates/process) ✔️
  - [/spin-mutex](../crates/spin-mutex) ✔️
  - [/time](../crates/time) ✔️
- [/drivers](../drivers)
  - [/ata](../drivers/ata) ✔️
  - [/pci](../drivers/pci) ✔️
  - [/pit](../drivers/pit) ✔️
  - [/rtc](../drivers/rtc) ✔️
  - [/uart-16550](../drivers/uart-16550) ✔️
  - [/virtio](../drivers/virtio) ✔️
  - [/x86-port](../drivers/x86-port) ✔️
- [/shell](../shell) ✔️
  - [/apps](../shell/apps)
    - [/input-driver](../shell/apps/input-driver) ✔️
- [/kernel](../kernel) ✔️
