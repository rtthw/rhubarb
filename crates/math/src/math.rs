//! # Math

#![no_std]

mod area;
mod point;
mod size;

pub use {area::*, point::*, size::*};



#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Axis {
    Horizontal,
    Vertical,
}

impl Axis {
    pub const fn cross(&self) -> Self {
        match self {
            Self::Horizontal => Self::Vertical,
            Self::Vertical => Self::Horizontal,
        }
    }

    #[inline]
    pub const fn pack_point(self, axis_value: f32, cross_value: f32) -> Point {
        match self {
            Self::Horizontal => Point::new(axis_value, cross_value),
            Self::Vertical => Point::new(cross_value, axis_value),
        }
    }

    #[inline]
    pub const fn pack_size(self, axis_value: f32, cross_value: f32) -> Size {
        match self {
            Self::Horizontal => Size::new(axis_value, cross_value),
            Self::Vertical => Size::new(cross_value, axis_value),
        }
    }
}



// HACK: Everything below is absolutely horrible and only exists because I
//       haven't added support for automatically aliased ELF sections yet. See
//       `kernel/loader::init` for more information.

#[unsafe(export_name = "__ltsf2")]
#[inline(never)]
pub extern "C" fn __ltsf2(a: f32, b: f32) -> CmpResult {
    __lesf2(a, b)
}

// Shamelessly copied from:
//  https://github.com/rust-lang/rust/blob/1d72d7e8136faaebad3a85eeed432e6ea1b2ffab/library/compiler-builtins/compiler-builtins/src/float/cmp.rs#L68
#[unsafe(export_name = "__lesf2")]
pub extern "C" fn __lesf2(a: f32, b: f32) -> CmpResult {
    let abs_mask = F32_SIGN_MASK - 1;

    let a_rep = a.to_bits();
    let b_rep = b.to_bits();
    let a_abs = a_rep & abs_mask;
    let b_abs = b_rep & abs_mask;

    // If either a or b is NaN, they are unordered.
    if a_abs > F32_EXP_MASK || b_abs > F32_EXP_MASK {
        return CMP_RES_UNORDERED;
    }

    // If a and b are both zeros, they are equal.
    if a_abs | b_abs == 0 {
        return CMP_RES_EQUAL;
    }

    let a_srep = a.to_bits().cast_signed();
    let b_srep = b.to_bits().cast_signed();

    // If at least one of a and b is positive, we get the same result comparing
    // a and b as signed integers as we would with a fp_ting-point compare.
    if a_srep & b_srep >= 0 {
        if a_srep < b_srep {
            CMP_RES_LESS
        } else if a_srep == b_srep {
            CMP_RES_EQUAL
        } else {
            CMP_RES_GREATER
        }
    // Otherwise, both are negative, so we need to flip the sense of the
    // comparison to get the correct result.  (This assumes a twos- or ones-
    // complement integer representation; if integers are represented in a
    // sign-magnitude representation, then this flip is incorrect).
    } else if a_srep > b_srep {
        CMP_RES_LESS
    } else if a_srep == b_srep {
        CMP_RES_EQUAL
    } else {
        CMP_RES_GREATER
    }
}

#[cfg(target_arch = "aarch64")]
pub type CmpResult = i32;
#[cfg(target_arch = "x86_64")]
pub type CmpResult = isize;

const F32_SIGN_MASK: u32 = 0x8000_0000;
const F32_EXP_MASK: u32 = 0x7f80_0000;

const CMP_RES_LESS: CmpResult = -1;
const CMP_RES_EQUAL: CmpResult = 0;
const CMP_RES_GREATER: CmpResult = 1;
const CMP_RES_UNORDERED: CmpResult = 1;
