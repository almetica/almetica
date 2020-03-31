/// This module provides the network packet definitions.

// Used in unit tests for de- and serialization.
#[allow(unused_macros)]
#[macro_export]
macro_rules! packet_test {
    (
        name: $name:ident,
        data: $data:expr,
        expected: $struct:expr
    ) => {
        #[test]
        fn $name() -> Result<(), Error> {
            let org = $data;
            let data = org.clone();
            let expected = $struct;
            assert_eq!(expected, from_vec(data)?);
            assert_eq!(org, to_vec(expected)?);
            Ok(())
        }
    };
}

#[macro_use]
pub mod client;
#[macro_use]
pub mod server;
