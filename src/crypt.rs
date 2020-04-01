/// Module that implements the cryptography used in TERA.
///
/// The stream cipher used by TERA is an implementation of the Pike streamcipher
/// as proposed by Ross Anderson in this paper:
///     https://www.cl.cam.ac.uk/~rja14/Papers/fibonacci.pdf
///
/// In this paper he explains the generation of the three lagged fibonacci key
/// generators used in this stream cipher and also the proposition to expand the
/// initial user provided key with the help of the SHA function.
///
/// Pike is a stream cipher based on the idea of a lagged Fibonacci generator.
/// This uses three arrays of u32 and relations similar to those in the
/// Fibonacci sequence, which is generated by successive applications
/// of xi = xi-1 + xi-2. In the cipher, however, the right side has more than
/// two terms and the coefficients are not just i-1 and i-2, but things like
/// i-7 or i-39. The coefficients and array size are different for each array.
/// Pike's clocking depends on the carry bits:
/// If all three carry bits match, all arrays are clocked and if only two match,
/// two are clocked.
///
/// The stream cipher output is the XOR of the three sums.
pub mod sha1;
pub mod streamcipher;

use streamcipher::StreamCipher;

// Represents the cryptography session between a client and a server.
// Direct port of the tera-network-proxy JS implementation to rust (GPL3).
// https://github.com/tera-toolbox/tera-network-proxy/blob/master/lib/connection/encryption/index.js
pub struct CryptSession {
    server_packet_cipher: StreamCipher,
    client_packet_cipher: StreamCipher,
}

impl CryptSession {
    /// Construct a `StreamCipherSession` object. Needs client and server keys.
    pub fn new(client_keys: [Vec<u8>; 2], server_keys: [Vec<u8>; 2]) -> CryptSession {
        let mut tmp1 = vec![0; 128];
        let mut tmp2 = vec![0; 128];
        let mut tmp3 = vec![0; 128];

        shift_key(&mut tmp1, &server_keys[0], -67);
        xor_key(&mut tmp2, &tmp1, &client_keys[0]);

        shift_key(&mut tmp1, &client_keys[1], 29);
        xor_key(&mut tmp3, &tmp1, &tmp2);
        let mut client_packet_cipher = StreamCipher::new(&tmp3);

        shift_key(&mut tmp1, &server_keys[1], -41);
        client_packet_cipher.apply_keystream(&mut tmp1);
        let server_packet_cipher = StreamCipher::new(&tmp1);

        CryptSession {
            server_packet_cipher,
            client_packet_cipher,
        }
    }

    /// Applies the StreamCipher for client packets on the given data and advances the state of the StreamCipher.
    /// To decrypt, you need to use a StreamCipher in the same state (look at the tests for an explanation).
    #[inline]
    pub fn crypt_client_data(&mut self, data: &mut [u8]) {
        self.client_packet_cipher.apply_keystream(data);
    }

    /// Applies the StreamCipher for server packets on the given data and advances the state of the StreamCipher.
    /// To decrypt, you need to use a StreamCipher in the same state (look at the tests for an explanation).
    #[inline]
    pub fn crypt_server_data(&mut self, data: &mut [u8]) {
        self.server_packet_cipher.apply_keystream(data);
    }
}

fn shift_key(dst: &mut [u8], src: &[u8], n: i32) {
    dst.copy_from_slice(src);
    if n > 0 {
        dst.rotate_left(n as usize)
    } else {
        dst.rotate_right(-n as usize)
    }
}

fn xor_key(dst: &mut [u8], key1: &[u8], key2: &[u8]) {
    for i in 0..128 {
        dst[i] = key1[i] ^ key2[i]
    }
}

#[cfg(test)]
mod tests {
    use super::CryptSession;
    use hex::{decode, encode};

    fn setup_session() -> CryptSession {
        let c1: Vec<u8> = vec![0x12; 128];
        let c2: Vec<u8> = vec![0x34; 128];
        let s1: Vec<u8> = vec![0x56; 128];
        let s2: Vec<u8> = vec![0x78; 128];

        return CryptSession::new([c1, c2], [s1, s2]);
    }

    #[test]
    fn test_client_packet_cipher() {
        let mut server_session = setup_session();
        let mut client_session = setup_session();

        let org: [u8; 32] = [0xFE; 32];
        let mut data: [u8; 32] = org;

        client_session.crypt_client_data(&mut data);
        server_session.crypt_client_data(&mut data);

        assert_eq!(encode(&org), encode(&data));
    }

    #[test]
    fn test_server_packet_cipher() {
        let mut server_session = setup_session();
        let mut client_session = setup_session();

        let org: [u8; 32] = [0xFE; 32];
        let mut data: [u8; 32] = org;

        server_session.crypt_server_data(&mut data);
        client_session.crypt_server_data(&mut data);

        assert_eq!(encode(&org), encode(&data));
    }

    #[test]
    fn test_server_packet_algorithm() {
        let mut client_session = setup_session();

        let org: [u8; 32] = [0xFE; 32];
        let mut data: [u8; 32] = org;
        client_session.crypt_server_data(&mut data);

        assert_eq!(
            "4e089f08f20dbae0c5b3af03871f464f0af7477149de07d1e3b466ecba521e62",
            encode(&data)
        );
    }

    #[test]
    fn test_client_packet_algorithm() {
        let mut server_session = setup_session();

        let org: [u8; 32] = [0xFE; 32];
        let mut data: [u8; 32] = org;
        server_session.crypt_client_data(&mut data);

        assert_eq!(
            "659f3e8745d2fcb73923bef592f99537acf4f96ac853fcbaa51bbbd4c62b9ded",
            encode(&data)
        );
    }

    #[test]
    fn test_real_session() {
        let c1: Vec<u8> = decode("179ac98624bccb8652ec52f15167b299412221ac2369c97e8dc61c89a22468b34234abcf7e10a8654f1f3ab68738e9fb070abbee33bb4c8ba3cf21181b4c3a63be1be64b7eb2dc81d7f08690bd48a01bd3220266dd7eba1978db5e400957a195f31723020de98e625328de88dbe100820b339f1e0afc3bae74b27b09528ec4d2").unwrap();
        let c2: Vec<u8> = decode("926b17901a5189f396a3a9752deda3464f03011586c23493ff9750629d69484b329e81664480cb57b705e6b855ddcf3ca6d315286f63465cfb317aa004fc02d50364b6e4466e9063e4cd7a99b1fc7a2093a2f1d3dd01ae075f6eb0ba4adff136c99ba112589cc43b442b17bc80dc3f493fc48b8f6b6c5bf8c1ec7fbc8b48aeae").unwrap();
        let s1: Vec<u8> = decode("ec27cdbb821377b653d3393a6b2bd81cf6290ed6b4eb1cbe998849fca76d3c14f8ee900322753108244bc7a1c2fe3354cbcdbc2742797e4e7a7347a4b209402370a2ea3400799176550e82b62ca6217f3a368827bf639e52dfb49205ded3fed343f062e023adb20480d5e7f1d207b04d99ef05b05c148def6195c95d4974c6a2").unwrap();
        let s2: Vec<u8> = decode("a38277131c05d351aa6b7df9e42747c5ab794022190d9908aa31fa47baf62faef8780634b8f4629b124de4f0ada5eee63e660c8c5dd1c549c7481bc6ef5cbcdb5e43e9bb0d498a500b9b8343f9dd7174e129db3daf31cfd45f064718d0f467694544173e38886c546898169ceaf8c50a6fe415a67c87a5449509fa49aec03752").unwrap();

        let mut server_session = CryptSession::new([c1, c2], [s1, s2]);

        let org: Vec<u8> = decode("38ee9f57801cf41ba89fc6a6dc1e4ab39c140df12f64038bbb35a3b01b1e0373af207eee102b5ee524132b41ba88e548c95a9d20a503a8b88bd086f6388aa5386aef2b65d6951234ccdcd5b6579bb63fc4d497cfcec2ea45650cd587fae917e455f6d6f5d2d06cc2712f58e66440729812a9231a03076674e78e65288f0855db420dfd0cac").unwrap();
        let mut data: Vec<u8> = org;
        server_session.crypt_client_data(&mut data);

        assert_eq!(
            "2000bc4d0200080008001400000000001d8a05001400000001000000ce7b05006500629a1700330032000000000000060000002a23000072006f00790061006c004200750073006800340038003000360000004f5363474b746d7233736e676234313872466e484544574d547259536248613238306a76655a744365473754377058763748",
            encode(&data)
        );
    }
}
