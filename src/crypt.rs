/// Module that implements the cryptography used in Tera.
pub mod sha1;

use byteorder::{BigEndian, ByteOrder, LittleEndian};
use sha1::Sha1;

// Provides a struct for the custom encryption used by Tera.
// Direct port the the C++ implementation of tera-toolbox to rust (MIT).
// https://github.com/tera-toolbox/tera-network-crypto/blob/master/main.cpp
struct Cryptor {
    keys: [CryptorKey; 3],
    change_data: u32,
    change_len: usize,
}

impl Cryptor {
    /// Construct a `Cryptor` object. Key must be 128 byte in size.
    pub fn new(key: &[u8]) -> Cryptor {
        let mut c = Cryptor {
            keys: [
                CryptorKey::new(55, 31),
                CryptorKey::new(57, 50),
                CryptorKey::new(58, 39),
            ],
            change_data: 0,
            change_len: 0,
        };

        // Expand the given key
        let mut expanded_key = [0; 680];
        expanded_key[0] = 128;
        for i in 1..680 {
            expanded_key[i] = key[i % 128];
        }
        for i in (0..680).step_by(20) {
            let mut sha = Sha1::new();
            sha.update(&expanded_key);
            let hash = sha.hash().unwrap();
            for j in (0..20).step_by(4) {
                LittleEndian::write_u32(&mut expanded_key[i + j..], hash[j]);
            }
        }

        // Create the cryptor keys out of the expanded key
        for i in 0..55 {
            c.keys[0].buffer[i] = BigEndian::read_u32(&expanded_key[i..]);
        }
        for i in 0..57 {
            c.keys[1].buffer[i] = BigEndian::read_u32(&expanded_key[(i * 4 + 220)..]);
        }
        for i in 0..58 {
            c.keys[2].buffer[i] = BigEndian::read_u32(&expanded_key[(i * 4 + 448)..]);
        }
        c
    }

    // Applies the cryptor on the data. Asymetric operation. Needs different keypairs for encryption and decryption.
    // TODO speed optimization
    pub fn apply(&mut self, data: &mut [u8]) {
        let size = data.len();
        let pre = if size < self.change_len {
            size
        } else {
            self.change_len
        };

        if pre != 0 {
            for i in 0..pre {
                let shift = 8 * (4 - self.change_len + i);
                data[i] ^= (self.change_data >> shift) as u8;
            }
            self.change_len -= pre;
        }

        for i in (pre..size - 3).step_by(4) {
            self.do_round();
            for k in self.keys.iter() {
                data[i] ^= k.sum as u8;
                data[i+1] ^= (k.sum >> 8) as u8;
                data[i+2] ^= (k.sum >> 16) as u8;
                data[i+3] ^= (k.sum >> 24) as u8;
            }
        }

        let remain = (size - pre) & 3;
        if remain != 0 {
            self.do_round();
            self.change_data = 0;
            for k in self.keys.iter() {
                self.change_data ^= k.sum;
            }

            for i in 0..remain {
                data[size-remain+i] ^= (self.change_data >> (i * 8)) as u8;
            }

            self.change_len = 4 - remain;
        }
    }

    fn do_round(&mut self) {
        let result = self.keys[0].key & self.keys[1].key
            | self.keys[2].key & (self.keys[0].key | self.keys[1].key);
        for i in 0..3 {
            let k = &mut self.keys[i];
            if result == k.key {
                let t1 = k.buffer[k.pos1 as usize];
                let t2 = k.buffer[k.pos2 as usize];
                let t3 = if t1 <= t2 { t1 } else { t2 };
                k.sum = t1 + t2;
                k.key = if t3 > k.sum { 1 } else { 0 };
                k.pos1 = (k.pos1+1) % k.size as u32;
                k.pos2 = (k.pos2+1) % k.size as u32;
            }
        }
    }
}

/// The key structure of the encryption key used by Tera
struct CryptorKey {
    pub size: usize,
    pub pos1: u32,
    pub pos2: u32,
    pub max_pos: u32,
    pub key: u32,
    pub buffer: Vec<u32>,
    pub sum: u32,
}

impl CryptorKey {
    /// Construct a `CryptorKey` object
    pub fn new(size: usize, max_pos: u32) -> CryptorKey {
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

// Represents the crypto session between a client and a server.
// Direct port of the tera-proxy-game JS implementation to rust (GPL3)
// https://github.com/tera-toolbox/tera-network-proxy/blob/master/lib/connection/encryption/index.js
struct CryptorSession {
    decryptor: Cryptor,
    encryptor: Cryptor,
}

impl CryptorSession {
    /// Construct a `CryptorSession` object. Needs server and client keys.
    pub fn new(client_keys: [[u8; 128]; 2], server_keys: [[u8; 128]; 2]) -> CryptorSession {
        let mut tmp1: [u8; 128] = [0; 128];
        let mut tmp2: [u8; 128] = [0; 128];
        let mut tmp3: [u8; 128] = [0; 128];

        shift_key(&mut tmp1, &server_keys[0], -67);
        xor_key(&mut tmp2, &tmp1, &client_keys[0]);
        shift_key(&mut tmp1, &client_keys[1], 29);
        xor_key(&mut tmp3, &tmp1, &tmp2);
        let mut decryptor = Cryptor::new(&tmp3);

        shift_key(&mut tmp1, &server_keys[1], -41);
        decryptor.apply(&mut tmp1);

        let encryptor = Cryptor::new(&tmp1);

        let cs = CryptorSession {
            decryptor: decryptor,
            encryptor: encryptor,
        };
        cs
    }

    /// Encrypt the given data.
    pub fn encrypt(&mut self, data: &mut [u8]) {
        self.encryptor.apply(data);
    }

    /// Decrypt the given data.
    pub fn decrypt(&mut self, data: &mut [u8]) {
        self.decryptor.apply(data);
    }
}

fn shift_key(dst: &mut [u8], src: &[u8], n: i8) {
    if n > 0 {
        dst.copy_from_slice(&src[0..n as usize]);
        dst.copy_from_slice(&src[(128 - n as usize)..]);
    } else {
        dst.copy_from_slice(&src[0..(128 + n as usize)]);
        dst.copy_from_slice(&src[-n as usize..]);
    }
}

fn xor_key(dst: &mut [u8], key1: &[u8], key2: &[u8]) {
    for i in 0..128 {
        dst[i] = key1[i] ^ key2[i]
    }
}

#[cfg(test)]
mod tests {
    // TODO
}
