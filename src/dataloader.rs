/// Module to read data files
use crate::protocol::opcode::Opcode;
use crate::*;
use aes::Aes128;
use anyhow::ensure;
use byteorder::{ByteOrder, LittleEndian};
use cfb_mode::stream_cipher::{NewStreamCipher, StreamCipher};
use cfb_mode::Cfb;
use flate2::{Decompress, FlushDecompress};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::PathBuf;

/// Read the encrypted data of a data center file and decrypt/decompress it.
pub fn read_datacenter_file(key: &[u8], iv: &[u8], mut data: Vec<u8>) -> Result<Vec<u8>> {
    ensure!(
        key.len() == 16 && iv.len() == 16,
        "KEY and IV must be 128 bits long (16 bytes)"
    );

    let mut cipher = Cfb::<Aes128>::new_var(key, iv).unwrap();
    let mut decompressor = Decompress::new(true);

    // Decrypt
    for chunk in data.chunks_mut(1024) {
        cipher.decrypt(chunk);
    }

    // Read final size
    let size = LittleEndian::read_u32(&data[0..4]) as usize;

    // Deflate
    let mut buffer = Vec::with_capacity(size);
    decompressor.decompress_vec(&data[4..], &mut buffer, FlushDecompress::None)?;
    ensure!(
        decompressor.total_out() == size as u64,
        "Decompression was successful, but data is missing to finalize it"
    );
    Ok(buffer)
}

/// Load opcode mapping from a file (normal and reverse lookup)
pub fn load_opcode_mapping(data_path: &PathBuf) -> Result<(Vec<Opcode>, HashMap<Opcode, u16>)> {
    let mut path = data_path.clone();
    path.push("opcode.yaml");
    let file = File::open(path)?;
    let mut buffered = BufReader::new(file);
    let opcode_mapping = read_opcode_table(&mut buffered)?;
    let reverse_opcode_mapping = calculate_reverse_map(opcode_mapping.as_slice());

    Ok((opcode_mapping, reverse_opcode_mapping))
}

/// Read the opcode mapping file and returns the opcode table.
pub fn read_opcode_table<T: ?Sized>(reader: &mut T) -> Result<Vec<Opcode>>
where
    T: Read,
{
    let opcode_map: HashMap<Opcode, u16> = serde_yaml::from_reader(reader)?;
    let mut opcode_table: Vec<Opcode> = vec![Opcode::UNKNOWN; std::u16::MAX as usize + 1];
    for (key, value) in opcode_map.iter() {
        opcode_table[*value as usize] = *key;
    }
    Ok(opcode_table)
}

pub fn calculate_reverse_map(opcode_mapping: &[Opcode]) -> HashMap<Opcode, u16> {
    let mut c: i32 = -1;
    let mut reverse_opcode_mapping = opcode_mapping
        .iter()
        .map(|op| {
            c += 1;
            (*op, c as u16)
        })
        .collect::<HashMap<Opcode, u16>>();
    reverse_opcode_mapping.remove(&Opcode::UNKNOWN);
    reverse_opcode_mapping
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use aes::Aes128;
    use byteorder::{LittleEndian, WriteBytesExt};
    use cfb_mode::stream_cipher::{NewStreamCipher, StreamCipher};
    use cfb_mode::Cfb;
    use flate2::{Compress, Compression, FlushCompress};
    use rand::rngs::OsRng;
    use rand_core::RngCore;

    use super::super::protocol::opcode::Opcode;
    use super::super::*;
    use super::*;

    #[test]
    fn test_opcode_table_creation() -> Result<()> {
        let mut file = Vec::new();
        file.write_all(
            "
                C_UNEQUIP_ITEM: 1
                S_ANNOUNCE_MESSAGE: 100
                C_ADD_FRIEND: 65535
                "
            .as_bytes(),
        )?;

        let table = read_opcode_table(&mut file.as_slice())?;
        let reverse_map = calculate_reverse_map(table.as_slice());

        assert_eq!(table[1], Opcode::C_UNEQUIP_ITEM);
        assert_eq!(table[100], Opcode::S_ANNOUNCE_MESSAGE);
        assert_eq!(table[65535], Opcode::C_ADD_FRIEND);

        assert_eq!(1, reverse_map[&Opcode::C_UNEQUIP_ITEM]);
        assert_eq!(100, reverse_map[&Opcode::S_ANNOUNCE_MESSAGE]);
        assert_eq!(65535, reverse_map[&Opcode::C_ADD_FRIEND]);

        Ok(())
    }

    #[test]
    fn test_read_datacenter_file() -> Result<()> {
        let size = 1024 * 1024;
        let key = hex::decode("1A8ED266690CCF664A741C4CA9D4944E")?;
        let iv = hex::decode("527DE56BB0A2C60DA879A01B8194DC12")?;

        let test_data = create_test_data(key.as_slice(), iv.as_slice(), size)?;
        let data = read_datacenter_file(key.as_slice(), iv.as_slice(), test_data)?;

        assert_eq!(size, data.len());
        Ok(())
    }

    // Creates some testdata in the same structure as the TERA datacenter files.
    //
    // Write down the size of the original data as u32. Then use zlib deflate to compress the
    // data (with the zlib header) and append the data to the u32 size bytes.
    //
    // Then use AES CFB with the KEY and IV in the TERA client (changes every patch)
    // and crypt the data (CFB is a stream cipher).
    fn create_test_data(key: &[u8], iv: &[u8], size: usize) -> Result<Vec<u8>> {
        let mut original_data = vec![0u8; size];
        OsRng.fill_bytes(original_data.as_mut_slice());

        let mut cipher = Cfb::<Aes128>::new_var(key, iv).unwrap();
        let mut compressor = Compress::new(Compression::best(), true);

        let mut buffer = Vec::with_capacity(size + 2048);
        buffer.write_u32::<LittleEndian>(size as u32)?;

        compressor
            .compress_vec(original_data.as_slice(), &mut buffer, FlushCompress::Full)
            .unwrap();
        if compressor.total_in() != size as u64 {
            panic!("compression did not read all the data");
        }

        let compressed_size = buffer.len();
        if compressed_size <= 4 {
            panic!("didn't compress any data");
        }

        for chunk in buffer.chunks_mut(1024) {
            cipher.encrypt(chunk);
        }

        buffer.shrink_to_fit();
        Ok(buffer)
    }
}
