/// Module that implements the cryptography used in Tera.
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
    pub fn new(client_keys: [[u8; 128]; 2], server_keys: [[u8; 128]; 2]) -> CryptSession {
        let mut tmp1: [u8; 128] = [0; 128];
        let mut tmp2: [u8; 128] = [0; 128];
        let mut tmp3: [u8; 128] = [0; 128];

        shift_key(&mut tmp1, &server_keys[0], -67);
        xor_key(&mut tmp2, &tmp1, &client_keys[0]);

        shift_key(&mut tmp1, &client_keys[1], 29);
        xor_key(&mut tmp3, &tmp1, &tmp2);
        let mut server_packet_cipher = StreamCipher::new(&tmp3);

        shift_key(&mut tmp1, &server_keys[1], -41);
        server_packet_cipher.apply(&mut tmp1);
        let client_packet_cipher = StreamCipher::new(&tmp1);

        let cs = CryptSession {
            server_packet_cipher: server_packet_cipher,
            client_packet_cipher: client_packet_cipher,
        };
        cs
    }

    /// Applies the StreamCipher for client packets on the given data and advances the state of the StreamCipher.
    /// To decrypt, you need to use a StreamCipher in the same state (look at the tests for an explanation).
    #[inline]
    pub fn crypt_client_data(&mut self, data: &mut [u8]) {
        self.client_packet_cipher.apply(data);
    }

    /// Applies the StreamCipher for server packets on the given data and advances the state of the StreamCipher.
    /// To decrypt, you need to use a StreamCipher in the same state (look at the tests for an explanation).
    #[inline]
    pub fn crypt_server_data(&mut self, data: &mut [u8]) {
        self.server_packet_cipher.apply(data);
    }
}

fn shift_key(dst: &mut [u8], src: &[u8], n: i32) {
    dst.copy_from_slice(src);
    if n > 0 {
        dst.rotate_right(n as usize)
    } else {
        dst.rotate_left(-n as usize)
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
    use hex::encode;

    fn setup_session() -> CryptSession {
        let c1: [u8; 128] = [0x12; 128];
        let c2: [u8; 128] = [0x34; 128];
        let s1: [u8; 128] = [0x56; 128];
        let s2: [u8; 128] = [0x78; 128];

        return CryptSession::new([c1, c2], [s1, s2]);
    }

    #[test]
    fn test_client_packet_cipher() {
        let mut server_session = setup_session();
        let mut client_session = setup_session();

        let org: [u8; 32] = [0xFE; 32];
        let mut data: [u8; 32] = org;
        
        server_session.crypt_client_data(&mut data);
        client_session.crypt_client_data(&mut data);
        
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
    fn test_client_packet_algorithm() {
        let mut client_session = setup_session();

        let org: [u8; 32] = [0xFE; 32];
        let mut data: [u8; 32] = org;
        
        client_session.crypt_client_data(&mut data);
        
        assert_eq!("4e089f08f20dbae0c5b3af03871f464f0af7477149de07d1e3b466ecba521e62", encode(&data));
    }

    #[test]
    fn test_server_packet_algorithm() {
        let mut server_session = setup_session();

        let org: [u8; 32] = [0xFE; 32];
        let mut data: [u8; 32] = org;
   
        server_session.crypt_server_data(&mut data);

        assert_eq!("659f3e8745d2fcb73923bef592f99537acf4f96ac853fcbaa51bbbd4c62b9ded", encode(&data));
    }
}
