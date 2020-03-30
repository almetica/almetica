/// Events that are handled / emitted by the ECS.
/// 
/// Events can be either global or local. Global events are always send to the
/// global world ECS first.
/// 
use super::super::protocol::packet::client::*;
//use super::super::protocol::packet::server::*;

macro_rules! assemble_event {
    ($($ty:ident($($arg_name:ident: $arg_type:ident),*), is_global: $is_global_event:literal,)*) => {
        /// Event enum for all events.
        #[derive(Clone, Debug)]
        pub enum Event {
            $($ty {$($arg_name: $arg_type),*},)*
        }

        impl Event {
            pub fn is_global(&self) -> bool {
                match self {
                    $(Event::$ty{$($arg_name,)*} => $is_global_event,)*
                }
            }
        }
    };
}

assemble_event!{
    RequestLoginArbiter(packet: CLoginArbiter), is_global: true,
    RequestCheckVersion(packet: CCheckVersion), is_global: true,
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::super::Error;

    #[test]
    fn test_is_global() -> Result<(), Error> {
        let org = Event::RequestLoginArbiter {
            packet: CLoginArbiter{
                master_account_name: "test".to_string(),
                ticket: vec![],
                unk1: 0,
                unk2: 0,
                language: 0,
                patch_version: 0,
            }
        };
        assert_eq!(true, org.is_global());
        Ok(())
    }
}
