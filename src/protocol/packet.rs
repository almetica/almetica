/// This module provides the network packet definitions.
///
/// Packets should be defined logically. So if an array starts at the top with it's header, we will
/// define the list first. Generally speaking, first define the the arrays, the bytes and strings first.
/// After that all primitive values should be defined.
///
/// Since we are working with a Postgres Database, prefer the i8, i16, i32, i64 data types. Only the
/// GameId in some packets (which is the EntityId of the ECS) should be u64. Data parsed from the
/// datacenter files is also parsed with the signed variants.
///
pub use client::*;
pub use server::*;

/// Used in unit tests for de- and serialization.
#[allow(unused_macros)]
#[macro_export]
macro_rules! packet_test {
    (
        name: $name:ident,
        data: $data:expr,
        expected: $struct:expr
    ) => {
        #[test]
        fn $name() -> Result<()> {
            let org = $data;
            let data = org.clone();
            let expected = $struct;
            // FIXME: expected value needs to be on the right side (but then we need a type hint for the methods).
            assert_eq!(expected, from_vec::<_>(data)?);
            assert_eq!(org, to_vec(expected)?);
            Ok(())
        }
    };
}

/// For debugging only.
#[allow(unused_macros)]
#[macro_export]
macro_rules! print_packet_data_test {
    (
        name: $name:ident,
        struct: $struct:expr
    ) => {
        #[test]
        fn $name() -> Result<()> {
            let structure = $struct;
            let data = to_vec(structure)?;
            println!("{}", format!("{:#x?}", data));
            assert!(false);
            Ok(())
        }
    };
}

mod client;
mod server;
