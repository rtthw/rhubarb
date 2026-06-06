//! # Boot Information
//!
//! Shared by the boot loader and kernel.

#![no_std]

use core::{
    fmt,
    ops::{Deref, DerefMut},
    str,
};


pub const MAX_OBJECT_NAME_LEN: usize = 32;

/// Information passed from the boot loader to the kernel when the OS boots up.
#[derive(Debug)]
#[repr(C)]
pub struct BootInfo {
    pub rsdp_address: Option<u64>,
    pub kernel_start: usize,
    pub kernel_end: usize,
    pub memory_map: MemoryMap,
    pub root_object_map: RootObjectMap,
    pub display_info: DisplayInfo,
}

#[derive(Debug)]
#[repr(C)]
pub struct DisplayInfo {
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub format: PixelFormat,
    pub framebuffer_addr: u64,
    pub framebuffer_size: usize,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(C)]
pub enum PixelFormat {
    None = 0,
    Rgb = 1,
    Bgr = 2,
}

#[derive(Debug)]
#[repr(C)]
pub struct MemoryMap {
    ptr: *mut MemoryRegion,
    len: usize,
}

impl Deref for MemoryMap {
    type Target = [MemoryRegion];

    fn deref(&self) -> &Self::Target {
        unsafe { core::slice::from_raw_parts(self.ptr, self.len) }
    }
}

impl DerefMut for MemoryMap {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { core::slice::from_raw_parts_mut(self.ptr, self.len) }
    }
}

impl From<&'static mut [MemoryRegion]> for MemoryMap {
    fn from(regions: &'static mut [MemoryRegion]) -> Self {
        Self {
            ptr: regions.as_mut_ptr(),
            len: regions.len(),
        }
    }
}

impl From<MemoryMap> for &'static mut [MemoryRegion] {
    fn from(map: MemoryMap) -> &'static mut [MemoryRegion] {
        unsafe { core::slice::from_raw_parts_mut(map.ptr, map.len) }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct MemoryRegion {
    pub base: usize,
    pub size: usize,
    pub kind: MemoryRegionKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
#[repr(C)]
pub enum MemoryRegionKind {
    Free,
    Bootloader,
    Uefi(u32),
}

#[derive(Debug)]
#[repr(C)]
pub struct RootObjectMap {
    ptr: *mut RootObjectInfo,
    len: usize,
}

impl Deref for RootObjectMap {
    type Target = [RootObjectInfo];

    fn deref(&self) -> &Self::Target {
        unsafe { core::slice::from_raw_parts(self.ptr, self.len) }
    }
}

impl DerefMut for RootObjectMap {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { core::slice::from_raw_parts_mut(self.ptr, self.len) }
    }
}

impl From<&'static mut [RootObjectInfo]> for RootObjectMap {
    fn from(regions: &'static mut [RootObjectInfo]) -> Self {
        Self {
            ptr: regions.as_mut_ptr(),
            len: regions.len(),
        }
    }
}

impl From<RootObjectMap> for &'static mut [RootObjectInfo] {
    fn from(map: RootObjectMap) -> &'static mut [RootObjectInfo] {
        unsafe { core::slice::from_raw_parts_mut(map.ptr, map.len) }
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
#[repr(C)]
pub struct RootObjectInfo {
    pub name: [u8; MAX_OBJECT_NAME_LEN],
    pub addr: usize,
    pub size: usize,
}

impl RootObjectInfo {
    pub fn name_str(&self) -> &str {
        core::str::from_utf8(&self.name).unwrap().trim_matches('\0')
    }
}

impl fmt::Debug for RootObjectInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ObjectInfo")
            .field("name", &self.name_str())
            .field("addr", &self.addr)
            .field("size", &self.size)
            .finish()
    }
}
