/// Module to read data files
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::PathBuf;

use crate::protocol::opcode::Opcode;
use crate::*;

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
fn read_opcode_table<T: ?Sized>(reader: &mut T) -> Result<Vec<Opcode>>
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

fn calculate_reverse_map(opcode_mapping: &[Opcode]) -> HashMap<Opcode, u16> {
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
    use super::super::protocol::opcode::Opcode;
    use super::super::*;
    use super::*;
    use std::io::Write;

    #[test]
    fn test_opcode_table_creation() -> Result<()> {
        let mut file = Vec::new();
        file.write_all(
            "
        C_UNEQUIP_ITEM: 1
        S_ANNOUNCE_MESSAGE: 5
        C_ADD_FRIEND: 2
        "
            .as_bytes(),
        )?;

        let table = read_opcode_table(&mut file.as_slice())?;
        let reverse_map = calculate_reverse_map(table.as_slice());

        assert_eq!(table[1], Opcode::C_UNEQUIP_ITEM);
        assert_eq!(table[5], Opcode::S_ANNOUNCE_MESSAGE);
        assert_eq!(table[2], Opcode::C_ADD_FRIEND);

        assert_eq!(5, reverse_map[&Opcode::S_ANNOUNCE_MESSAGE]);
        assert_eq!(2, reverse_map[&Opcode::C_ADD_FRIEND]);
        assert_eq!(1, reverse_map[&Opcode::C_UNEQUIP_ITEM]);

        Ok(())
    }
}
