//! # File System (FS) Types

#![no_std]

extern crate alloc;

use alloc::{string::String, vec::Vec};



pub trait FileSystem {
    fn list(&self, dir_path: &str) -> Result<Vec<String>, &'static str>;
    fn read(&mut self, path: &str) -> Result<Vec<u8>, &'static str>;
}

pub struct DirectoryEntry {
    pub index: usize,
    pub name: String,
    pub size: usize,
}
