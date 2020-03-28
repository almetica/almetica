/// Module to read data files
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::PathBuf;

use super::*;
use super::protocol::opcode::Opcode;

/// Load opcode mapping from a file.
pub fn load_opcode_mapping(data_path: &PathBuf) -> Result<Vec<Opcode>> {
    let mut path = data_path.clone();
    path.push("opcode.yaml");
    let file = File::open(path)?;
    let mut buffered = BufReader::new(file);
    let opcodes = read_opcode_table(&mut buffered)?;
    Ok(opcodes)
}

/// Read the opcode mapping file and returns the opcode table.
fn read_opcode_table<T: ?Sized>(reader: &mut T) -> Result<Vec<Opcode>>
where
    T: Read,
{
    let opcode_map: HashMap<Opcode, u16> = serde_yaml::from_reader(reader)?;
    let mut opcode_table: Vec<Opcode> = vec![Opcode::UNKNOWN; std::u16::MAX as usize];
    for (key, value) in opcode_map.iter() {
        opcode_table[*value as usize] = key.clone();
    }
    Ok(opcode_table)
}

#[cfg(test)]
mod tests {
    use super::read_opcode_table;
    use super::super::protocol::opcode::Opcode;
    use std::io::Write;

    #[test]
    fn test_read_opcode_table() {
        let mut file = Vec::new();
        file.write_all("
        C_UNEQUIP_ITEM: 1
        S_ANNOUNCE_MESSAGE: 5
        C_ADD_FRIEND: 2
        ".as_bytes()).unwrap();

        let table = read_opcode_table(&mut file.as_slice()).unwrap();

        assert_eq!(table[1], Opcode::C_UNEQUIP_ITEM);
        assert_eq!(table[5], Opcode::S_ANNOUNCE_MESSAGE);
        assert_eq!(table[2], Opcode::C_ADD_FRIEND);
    }
}
