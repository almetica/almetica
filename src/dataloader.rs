/// Module to read data files
use std::io::Read;
use std::collections::HashMap;

use super::protocol::opcode::Opcode;
use anyhow::Result;

/// Read the opcode mapping file and returns the opcode table.
pub fn read_opcode_table<T: ?Sized>(reader: &mut T) -> Result<Vec<Opcode>>
where
    T: Read,
{
    let opcode_map: HashMap<Opcode, u16> = serde_yaml::from_reader(reader)?;
    let mut opcode_table: Vec<Opcode> = vec![Opcode::UNKNOWN; std::u16::MAX as usize];
    for (key, value) in opcode_map.iter() {
        opcode_table[*value as usize] = *key;
    }
    Ok(opcode_table)
}
