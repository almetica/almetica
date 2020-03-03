// Module that implements the cryptography used in Tera.
pub mod sha1;

use byteorder::{BigEndian, ByteOrder};

// Provides a struct for the custom encryption used by Tera.
// Direct port the the C++ implementation of tera-toolbox to rust.
// https://github.com/tera-toolbox/tera-network-crypto/blob/master/main.cpp
struct Cryptor{
    keys: [CryptorKey; 3],
}

impl Cryptor {

    /// Construct a `Cryptor` object
    pub fn new(key: &[u8]) -> Cryptor {
        let mut c = Cryptor {
            keys: [
                CryptorKey::new(55, 31),
                CryptorKey::new(57, 50),
                CryptorKey::new(58, 39),
            ]
        };
        for i in 0..55 {
            c.keys[0].buffer[i] = BigEndian::read_u32(&key[i..]);
        }
        for i in 0..57 {
            c.keys[1].buffer[i] = BigEndian::read_u32(&key[(i * 4 + 220)..]);
        }
        for i in 0..58 {
            c.keys[1].buffer[i] = BigEndian::read_u32(&key[(i * 4 + 448)..]);
        }
        c
    }

    pub fn encrypt(data: &mut [u8]) {

    }

    pub fn decrypt(data: &mut [u8]) {

    }

}

// The key structure of the encryption key used by Tera
struct CryptorKey {
	pub size: usize,
	pub pos1: i32,
	pub pos2: i32,
	pub max_pos: i32,
	pub key: i32,
	pub buffer: Vec<u32>,
	pub sum: u32,
}

impl CryptorKey {

    /// Construct a `CryptorKey` object
    pub fn new(size: usize, max_pos: i32) -> CryptorKey {
        let ck = CryptorKey {
            size: size,
            pos1: 0,
            pos2: max_pos,
            max_pos: max_pos,
            key: 0,
            buffer: vec![0; size],
            sum: 0,
        };
        ck
    }
}
