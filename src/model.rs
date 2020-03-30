/// Module that describes the models used for persistence.
///
/// Models should not directly exposed to the client.
/// Only enums can be shared freely.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
pub enum Region {
    International = 0,
    Korea = 1,
    Usa = 2,
    Japan = 3,
    Germany = 4,
    France = 5,
    Europe = 6,
    Taiwan = 7,
    Russia = 8,
}
