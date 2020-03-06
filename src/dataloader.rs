/// Module to read data files
use std::io::Read;
use std::collections::HashMap;

use super::*;
use super::protocol::opcode::Opcode;

/// Read the opcode mapping file and returns the opcode table.
pub fn read_opcode_table<T: ?Sized>(reader: &mut T) -> Result<Vec<Opcode>>
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
