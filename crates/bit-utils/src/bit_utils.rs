//! # Bit Manipulation Utilities

#![no_std]



#[macro_export]
macro_rules! bit_flags {
    (
        $(#[$meta:meta])*
        $vis:vis struct $ident:ident: $ty:ty {
            $(
                $(#[$flag_meta:meta])*
                $flag_ident:ident @ $flag_bit:expr
            ),*
            $(,)?
        }
    ) => {
        #[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
        #[allow(non_camel_case_types)]
        #[repr(transparent)]
        $(#[$meta])*
        $vis struct $ident($ty);

        #[allow(unused)]
        impl $ident {
            $(
                $(#[$flag_meta])*
                pub const $flag_ident: Self = Self(1 << $flag_bit);
            )*

            pub const NONE: Self = Self(0);
            pub const ALL: Self = Self(0 $(| 1 << $flag_bit)*);

            #[inline]
            pub const fn bits(&self) -> $ty {
                self.0
            }

            #[inline]
            pub const fn get(&self, other: Self) -> bool {
                self.0 & other.0 == other.0
            }
        }

        impl ::core::fmt::Debug for $ident {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                if self == &Self::NONE {
                    return write!(f, "NONE");
                }

                let mut first = true;
                $(
                    if *self & Self::$flag_ident != Self::NONE {
                        if first {
                            write!(f, stringify!($flag_ident))?;
                            first = false;
                        } else {
                            write!(f, concat!(" | ", stringify!($flag_ident)))?;
                        }
                    }
                )*

                Ok(())
            }
        }

        impl ::core::ops::Not for $ident {
            type Output = Self;

            fn not(self) -> Self::Output {
                Self(!self.0)
            }
        }

        impl ::core::ops::BitOr for $ident {
            type Output = Self;

            fn bitor(self, rhs: Self) -> Self::Output {
                Self(self.0 | rhs.0)
            }
        }

        impl ::core::ops::BitOrAssign for $ident {
            fn bitor_assign(&mut self, rhs: Self) {
                *self = Self(self.0 | rhs.0)
            }
        }

        impl ::core::ops::BitAnd for $ident {
            type Output = Self;

            fn bitand(self, rhs: Self) -> Self::Output {
                Self(self.0 & rhs.0)
            }
        }

        impl ::core::ops::BitAndAssign for $ident {
            fn bitand_assign(&mut self, rhs: Self) {
                *self = Self(self.0 & rhs.0)
            }
        }

        impl ::core::ops::BitXor for $ident {
            type Output = Self;

            fn bitxor(self, rhs: Self) -> Self::Output {
                Self(self.0 ^ rhs.0)
            }
        }

        impl ::core::ops::BitXorAssign for $ident {
            fn bitxor_assign(&mut self, rhs: Self) {
                *self = Self(self.0 ^ rhs.0)
            }
        }
    };
}

#[macro_export]
macro_rules! bit_field {
    (
        $(#[$meta:meta])*
        $vis:vis struct $ident:ident: $ty:ty {
            $($fields:tt)*
        }
    ) => {
        #[derive(Clone, Copy, Eq, PartialEq)]
        #[repr(transparent)]
        $(#[$meta])*
        $vis struct $ident($ty);

        impl $ident {
            $crate::bit_field!(@_field $($fields)*);
        }
    };

    // Ranged field.
    (@_field
        $(#[$field_meta:meta])*
        $field_vis:vis $field_ident:ident: $field_ty:ty
            = $field_range_start:literal..$field_range_end:literal

        $(, $($rest:tt)*)?
    ) => {
        $(#[$field_meta])*
        $field_vis const fn $field_ident(&self) -> $field_ty {
            let num = self.0;
            $crate::bit_range!(num[$field_range_start..$field_range_end]) as $field_ty
        }

        $(
            $crate::bit_field!(@_field $($rest)*);
        )?
    };

    // Single-bit field.
    (@_field
        $(#[$field_meta:meta])*
        $field_vis:vis $field_ident:ident: $field_ty:ty = $field_bit:literal

        $(, $($rest:tt)*)?
    ) => {
        $(#[$field_meta])*
        $field_vis const fn $field_ident(&self) -> $field_ty {
            let num = self.0;
            $crate::bit_range!(num[$field_bit])
        }

        $(
            $crate::bit_field!(@_field $($rest)*);
        )?
    };

    (@_field) => {};
}

#[macro_export]
macro_rules! bit_range {
    ($num:ident[$($start:ident)?..$($end:expr)?] $(as $ty:ty)?) => {{
        let width = $num.count_ones() + $num.count_zeros();
        let start = $crate::__expand_if_empty!($($start)? ; 0);

        $crate::bit_range!(
            @_done $num ; width ;
            start ;
            $crate::__expand_if_empty!($($end)? ; {
                start + $crate::__expand_if_empty!($(<$ty>::BITS)? ; width)
            })
        )
    }};
    ($num:ident[$($start:literal)?..$($end:expr)?] $(as $ty:ty)?) => {{
        let width = $num.count_ones() + $num.count_zeros();
        let start = $crate::__expand_if_empty!($($start)? ; 0);
        let end = $crate::__expand_if_empty!($($end)? ; {
            start + $crate::__expand_if_empty!($(<$ty>::BITS)? ; width)
        });

        $crate::bit_range!(@_done $num ; width ; start ; end )
    }};

    ($num:ident[$index:expr] $(as $ty:ty)?) => {{
        $num & (1 << $index) != 0
    }};

    (@_done $num:expr ; $width:expr ; $start:expr ; $end:expr) => {{
        if $start == $end {
            $num & (1 << $start)
        } else {
            let bits = $num << $width.saturating_sub($end) >> $width.saturating_sub($end);
            bits >> $start
        }
    }};
}

/// Internal utility macro that evaluates to the provided expansion if the input
/// before the semicolon is empty.
///
/// ## Examples
///
/// ```
/// use bit_utils::__expand_if_empty;
/// assert_eq!(__expand_if_empty!(0;1), 0);
/// assert_eq!(__expand_if_empty!( ;1), 1);
/// ```
#[macro_export]
#[doc(hidden)]
macro_rules! __expand_if_empty {
    ($something:expr ; $expansion:expr) => {
        $something
    };
    (; $expansion:expr) => {
        $expansion
    };
}



#[cfg(test)]
mod tests {
    #[test]
    #[rustfmt::skip]
    fn range_smoke() {
        let num: u64 = 43;
        assert_eq!(bit_range!(num[5..31] as u16), 1);

        let num: u8 = 0b_0011_0101;
        assert_eq!(bit_range!(num[0..0]), 0b_000001);
        assert_eq!(bit_range!(num[0..3]), 0b_000101);
        assert_eq!(bit_range!(num[2..6]), 0b_001101);
        assert_eq!(bit_range!(num[ .. ]), 0b_110101);

        let num: u32 = 0b_1001_0001;
        assert_eq!(bit_range!(num[4..4]), 0b_010000);
        assert_eq!(bit_range!(num[ ..3]), 0b_000001);
        assert_eq!(bit_range!(num[2.. ]), 0b_100100);
        assert_eq!(bit_range!(num[ ..5]), 0b_010001);

        // Make sure it works on constants.
        const NUM: u16 = 0b_1011_1111_0000_1101;
        assert_eq!(bit_range!(NUM[1..9 ] as u8), 0b_1000_0110);
        assert_eq!(bit_range!(NUM[8..  ] as u8), 0b_1011_1111);
        assert_eq!(bit_range!(NUM[4..12] as u8), 0b_1111_0000);

        const SOURCE_NUM: u32 = 0x_FEFE_FEFE;
        const RANGE_START: u32 = 2;
        const RANGE_END: u32 = 7;
        const RANGED_NUM: u32 = bit_range!(SOURCE_NUM[RANGE_START..RANGE_END] as u32);
        assert_eq!(RANGED_NUM, 31);
        assert_eq!(bit_range!(RANGED_NUM[0]), true);
        assert_eq!(bit_range!(RANGED_NUM[1]), true);
        assert_eq!(bit_range!(RANGED_NUM[2]), true);
        assert_eq!(bit_range!(RANGED_NUM[3]), true);
        assert_eq!(bit_range!(RANGED_NUM[4]), true);
        assert_eq!(bit_range!(RANGED_NUM[5]), false);
    }

    #[test]
    fn flags_smoke() {
        bit_flags! {
            /// Flag docs.
            struct Flags: u8 {
                /// Docs for A.
                A @ 0,
                /// Docs for B.
                B @ 2,
                /// Docs for C.
                C @ 7,
            }
        }

        assert_eq!(Flags::ALL, Flags::A | Flags::B | Flags::C);

        assert_eq!((Flags::A | Flags::B).bits(), 0b0000_0101);
        assert_eq!((Flags::B | Flags::C).bits(), 0b1000_0100);
        assert_eq!((Flags::A | Flags::C).bits(), 0b1000_0001);
    }

    #[test]
    #[rustfmt::skip]
    fn field_smoke() {
        bit_field! {
            /// See: https://wiki.osdev.org/HPET#General_Capabilities_and_ID_Register
            struct HpetCaps: u64 {
                /// Indicates which revision of the function is implemented; must not be 0.
                revision: u8        = 0..8,
                /// The amount of timers - 1.
                timer_count: u8     = 8..13,
                /// If this bit is 1, HPET main counter is capable of operating in 64 bit mode.
                count_size: bool    = 13,
                /// If this bit is 1, HPET is capable of using "legacy replacement" mapping.
                legacy_rep: bool    = 15,
                /// This field should be interpreted similarly to PCI's vendor ID.
                vendor_id: u16      = 16..32,
                /// Main counter tick period in femtoseconds (10^-15 seconds). Must not be zero,
                /// must be less or equal to 0x05F5E100, or 100 nanoseconds.
                period: u32         = 32..64,
            }
        }

        let test_caps = HpetCaps(
            (43 << 32)      // period
            + (29 << 16)    // vendor_id
            + (1 << 13)     // count_size
            + (0 << 15)     // legacy_rep
            + (7 << 8)      // timer_count
            + (2 << 0),     // revision
        );

        assert_eq!(test_caps.period(), 43);
        assert_eq!(test_caps.vendor_id(), 29);
        assert_eq!(test_caps.legacy_rep(), false);
        assert_eq!(test_caps.count_size(), true);
        assert_eq!(test_caps.timer_count(), 7);
        assert_eq!(test_caps.revision(), 2);
    }
}
