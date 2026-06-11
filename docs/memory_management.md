
> [!WARNING]
> This document is a DRAFT. Information may be inaccurate or incomplete.

<details>
<summary>Table of Contents</summary>

- [Memory Management](#memory-management)
  - [A brief note on physical memory](#a-brief-note-on-physical-memory)

</details>

# Memory Management

## A brief note on physical memory

Because device drivers often need to be able to directly access physical memory and run in userspace, **all physical memory is identity-mapped in userspace**. This means **there is no distinction between physical and virtual memory**.

However, physical memory is not flagged as accessible to users (and, for performance reasons, not *actually* mapped until it is accessed). When user processes first attempt to access an address in physical memory, the CPU will generate a page fault. During this page fault, the kernel determines the faulting process's permissions and will either choose to update the accessed page's flags to allow access if the process has permission or kill the process if it does not.

> [!NOTE]
> On x86, physical memory is limited to 52 bits and virtual addresses must be sign-extended past the 47th bit. This means the top 12 bits of each physical address must always be 0 and the top 16 bits of each virtual address must match the 47th bit. Because all physical memory is identity-mapped and must be accessible from a virtual address, the largest valid physical address is `0x7FFF_FFFF_FFFF`. Rhubarb therefore only supports 128 TiB of physical memory.
