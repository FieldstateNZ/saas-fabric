//! Every variant of a published enum, from one table.
//!
//! [`every_variant!`] takes one list of variants, each with a specimen value
//! for every field, and generates from that same list both an exhaustive
//! `match` and the specimens. A variant added to an enum does not compile
//! until it has an entry, and an entry is a specimen, so the writer's
//! output for it reaches the lock whether or not it deserializes.
//!
//! A variant the writer cannot write goes under `unwritten`: it is matched
//! too, and serializing it must fail, so leaving a variant out of the
//! specimens is proved rather than assumed.

/// `every_variant!(Enum [written variants] unwritten [variants])`, each
/// variant as `Name` or `Name { field: specimen, ... }`. Evaluates to the
/// written variants, in order.
macro_rules! every_variant {
    (
        $ty:ident
        [ $( $variant:ident $( { $( $field:ident : $value:expr ),* $(,)? } )? ),* $(,)? ]
        unwritten
        [ $( $unwritten:ident $( { $( $ufield:ident : $uvalue:expr ),* $(,)? } )? ),* $(,)? ]
    ) => {{
        fn covered(variant: &$ty) {
            match variant {
                $( $ty::$variant $( { $( $field: _ ),* } )? => {} )*
                $( $ty::$unwritten $( { $( $ufield: _ ),* } )? => {} )*
            }
        }
        let written = vec![ $( $ty::$variant $( { $( $field: $value ),* } )? ),* ];
        written.iter().for_each(covered);
        $(
            let unwritten = $ty::$unwritten $( { $( $ufield: $uvalue ),* } )?;
            covered(&unwritten);
            assert!(
                serde_json::to_value(&unwritten).is_err(),
                "{} is listed as unwritten, but the writer serializes it",
                stringify!($unwritten)
            );
        )*
        written
    }};
}

pub(crate) use every_variant;
