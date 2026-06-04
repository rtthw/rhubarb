
> [!WARNING]
> This document is a DRAFT. Information may be inaccurate or incomplete.

<details>
<summary>Table of Contents</summary>

- [Memory Management](#memory-management)
  - [Physical Memory](#physical-memory)

</details>

# Memory Management

## Physical Memory

**All physical memory is identity-mapped in userspace**. However, none of it is flagged as accessible to users. When user processes first attempt to access an address in physical memory, the CPU will generate a page fault. During this page fault, the kernel determines the process's permissions and will either choose to update the accessed page's flags to allow access if the process has permission or kill the process if it does not.

### Limit

On x86, physical memory is limited to 52 bits and virtual addresses must be sign-extended past the 47th bit. This means the top 12 bits of each physical address must always be 0 and the top 16 bits of each virtual address must match the 47th bit. Because all physical memory is identity-mapped and must be accessible from a virtual address, the maximum physical address is `0x7FFF_FFFF_FFFF`. Rhubarb therefore only supports 128 TiB of physical memory.
