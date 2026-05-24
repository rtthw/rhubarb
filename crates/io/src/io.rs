//! # Input/Output (IO)

#![no_std]

use core::fmt;



#[derive(Debug)]
pub enum ReadError {
    /// The length of `buffer` is not valid for the reader.
    InvalidBufferLength { length: usize },
    /// The given offset was past the end of the reader's buffer.
    OffsetPastEnd { offset: usize, max_offset: usize },
    /// The read operation timed out.
    TimedOut,
    /// Some implementation-specific error not covered by one of the other
    /// variants.
    Other(&'static str),
}

#[derive(Debug)]
pub enum WriteError {
    /// The given offset was past the end of the writer's buffer.
    OffsetPastEnd { offset: usize, max_offset: usize },
    /// The write operation timed out.
    TimedOut,
    /// Some implementation-specific error not covered by one of the other
    /// variants.
    Other(&'static str),
}

impl core::error::Error for ReadError {}
impl core::error::Error for WriteError {}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidBufferLength { length } => write!(f, "invalid buffer length {length}"),
            Self::OffsetPastEnd { offset, max_offset } => write!(
                f,
                "offset {offset} greater than size of read buffer {max_offset}",
            ),
            Self::TimedOut => write!(f, "timed out"),
            Self::Other(message) => write!(f, "{}", message),
        }
    }
}

impl fmt::Display for WriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OffsetPastEnd { offset, max_offset } => write!(
                f,
                "offset {offset} greater than size of write buffer {max_offset}",
            ),
            Self::TimedOut => write!(f, "timed out"),
            Self::Other(message) => write!(f, "{}", message),
        }
    }
}

impl From<&'static str> for ReadError {
    fn from(value: &'static str) -> Self {
        Self::Other(value)
    }
}

impl From<&'static str> for WriteError {
    fn from(value: &'static str) -> Self {
        Self::Other(value)
    }
}



pub trait Flush {
    /// Flush the writer's contents.
    fn flush(&mut self) -> Result<(), &'static str>;
}

pub trait BlockSize {
    fn block_size(&self) -> usize;
}

pub trait BlockReader: BlockSize {
    fn read_blocks(&mut self, offset: usize, buffer: &mut [u8]) -> Result<usize, ReadError>;
}

pub trait BlockWriter: Flush + BlockSize {
    fn write_blocks(&mut self, offset: usize, buffer: &[u8]) -> Result<usize, WriteError>;
}
