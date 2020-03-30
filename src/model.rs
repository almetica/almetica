/// Module that describes the models used for persistence.
///
/// Only the simple enums and data structures should be shared with the
/// client.
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

// type angle = i16;

// struct vec3 {
//     x: f32,
//     y: f32,
//     z: f32,
// }

// struct vec3a {
//     x: i32,
//     y: i32,
//     z: i32,
// }

// type skill_id = [u8; 4]; // Patch < 74
// type skill_id = [u8; 8]; // Path >= 74

// type customization = [u8; 4];
