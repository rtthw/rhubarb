//! The bare minimum needed for linking/loading types in [the `alloc`
//! crate](alloc).
//!
//! References:
//!
//! - https://github.com/rust-lang/compiler-team/issues/858
//! - https://github.com/rust-lang/rust/pull/86844


// It took me an incredibly long time to figure out how to get these symbols to
// link correctly. DO NOT CHANGE!


#[unsafe(export_name = "__rustc::__rust_no_alloc_shim_is_unstable_v2")]
pub fn __rust_no_alloc_shim_is_unstable_v2() {}

#[rustc_std_internal_symbol]
pub fn __rust_alloc_error_handler_should_panic() -> u8 {
    0
}

#[rustc_std_internal_symbol]
pub fn __rust_alloc_error_handler(_size: usize, _align: usize) {}
