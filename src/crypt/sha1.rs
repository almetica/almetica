/// Module that implements the SHA1 variant used in TERA.
///
/// TERA's SHA1 implementation is close to the original SHA1 algorithm, but with two differences: expanded values
/// aren't rotated and the output u32 words are little-endian.
use byteorder::{BigEndian, ByteOrder};

/// Structure representing the state of a SHA1 computation
/// Direct port the the JS implementation of tera-proxy to rust (MIT).
/// https://github.com/tera-toolbox/tera-network-proxy/blob/master/lib/connection/encryption/sha0.js
pub struct Sha1 {
    digest: [u32; 5],
    block: [u8; 64],
    block_index: usize,
    length: u64,
    computed: bool,
}

impl Sha1 {
    /// Construct a `Sha1` object
    pub fn new() -> Sha1 {
        Default::default()
    }

    /// Update the hash with new data
    pub fn update(&mut self, data: &[u8]) {
        for b in data {
            self.block[self.block_index] = *b;
            self.block_index += 1;
            self.length += 8;
            if self.block_index == 64 {
                self.process_message_block();
            }
        }
    }

    /// Calculate the final hash
    pub fn hash(&mut self) -> Result<[u32; 5], std::io::Error> {
        if !self.computed {
            self.pad_message();
            self.computed = true;
        }

        Ok(self.digest)
    }

    fn process_message_block(&mut self) {
        let mut words: [u32; 80] = [0; 80];

        // Break chunk into sixteen u32 big-endian words
        for (i, el) in words.iter_mut().take(16).enumerate() {
            *el = BigEndian::read_u32(&self.block[i * 4..]);
        }

        // Message schedule: extend the sixteen u32 into eighty u32
        for i in 16..80 {
            words[i] = words[i - 3] ^ words[i - 8] ^ words[i - 14] ^ words[i - 16];
        }

        // Initialize hash value for this chunk
        let mut a = self.digest[0];
        let mut b = self.digest[1];
        let mut c = self.digest[2];
        let mut d = self.digest[3];
        let mut e = self.digest[4];

        // Main loop
        for (i, el) in words.iter().enumerate() {
            let mut temp = e.wrapping_add(left_rotate(a, 5)).wrapping_add(*el);
            if i < 20 {
                temp = temp.wrapping_add((b & c) | ((!b) & d));
                temp = temp.wrapping_add(consts::K[0]);
            } else if i < 40 {
                temp = temp.wrapping_add(b ^ c ^ d);
                temp = temp.wrapping_add(consts::K[1]);
            } else if i < 60 {
                temp = temp.wrapping_add((b & c) | (b & d) | (c & d));
                temp = temp.wrapping_add(consts::K[2]);
            } else {
                temp = temp.wrapping_add(b ^ c ^ d);
                temp = temp.wrapping_add(consts::K[3]);
            }
            e = d;
            d = c;
            c = left_rotate(b, 30);
            b = a;
            a = temp;
        }

        // Add this chunk's hash to result so far
        self.digest[0] = self.digest[0].wrapping_add(a);
        self.digest[1] = self.digest[1].wrapping_add(b);
        self.digest[2] = self.digest[2].wrapping_add(c);
        self.digest[3] = self.digest[3].wrapping_add(d);
        self.digest[4] = self.digest[4].wrapping_add(e);

        self.block_index = 0;
    }

    fn pad_message(&mut self) {
        // Check to see if the current message block is too small to hold
        // the initial padding bits and length.  If so, we will pad the
        // block, process it, and then continue padding into a second
        // block.
        self.block[self.block_index] = 0x80;
        self.block_index += 1;

        if self.block_index > 55 {
            for i in self.block_index..64 {
                self.block[i] = 0;
                self.block_index += 1;
            }
            self.process_message_block();
        }

        if self.block_index < 56 {
            for i in self.block_index..56 {
                self.block[i] = 0;
                self.block_index += 1;
            }
        }

        BigEndian::write_u64(&mut self.block[56..], self.length);

        self.process_message_block();
    }
}

impl Default for Sha1 {
    fn default() -> Self {
        Sha1 {
            digest: consts::H,
            block: [0; 64],
            block_index: 0,
            length: 0,
            computed: false,
        }
    }
}

#[inline]
fn left_rotate(word: u32, shift: u32) -> u32 {
    (word << shift) | (word >> (32 - shift))
}

mod consts {
    pub const H: [u32; 5] = [
        0x6745_2301,
        0xefcd_ab89,
        0x98ba_dcfe,
        0x1032_5476,
        0xc3d2_e1f0,
    ];
    pub const K: [u32; 4] = [0x5a82_7999, 0x6ed9_eba1, 0x8f1b_bcdc, 0xca62_c1d6];
}

#[cfg(test)]
mod tests {
    use byteorder::{ByteOrder, LittleEndian};
    use hex::encode;

    use super::*;

    // Helper function
    fn digest_to_hex(msg: &str) -> String {
        let mut h = Sha1::new();
        h.update(&msg.as_bytes());
        let hash = h.hash().unwrap();
        let mut buf = [0; 20];
        for i in 0..5 {
            LittleEndian::write_u32(&mut buf[i * 4..], hash[i])
        }
        encode(buf)
    }

    #[test]
    fn test_sha1_empty() {
        assert_eq!(
            digest_to_hex(""),
            "19ea6cf956ddd18a4a08ac1710c6923defc00877",
        );
    }

    #[test]
    fn test_sha1_hello_world() {
        assert_eq!(
            digest_to_hex("hello world"),
            "c382ce9f95c18748a2b3403b85183e88a6a84f0c",
        );
        assert_eq!(
            digest_to_hex("hello, world"),
            "cd4df1db2c067776df20233f305e1c8bb9101d94",
        );
        assert_eq!(
            digest_to_hex("Hello, World"),
            "8a3e3ab2ba039d638aa171b17a1a477b06d19b53",
        );
    }
}
