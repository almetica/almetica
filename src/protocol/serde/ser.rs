use std::collections::HashMap;

use super::{Error, Result};
use byteorder::{ByteOrder, LittleEndian};
use serde::{ser, Serialize};

#[derive(Debug, Clone)]
pub struct Serializer {
    current_node: usize,
    nodes: HashMap<usize, DataNode>,
}

#[derive(Debug, Clone)]
struct DataNode {
    node_type: DataNodeType,
    parent: usize,
    childs: Vec<usize>,
    element_offsets: Vec<usize>,
    data: Vec<u8>,
    parent_offset: usize,
}

#[derive(Debug, Clone, Copy)]
enum DataNodeType {
    Root,
    Array,
    Bytes,
    String,
}

impl Serializer {
    /// Calculates the length to use for offset calculations (only offsets of depth 0 are absolute)
    fn calculate_length(&self, depth: usize, node_length: usize, parent_length: usize) -> usize {
        if depth == 0 {
            node_length + parent_length
        } else {
            node_length
        }
    }

    /// Recursively assemble to data nodes into one packet
    fn assemble_node(&mut self, num_node: usize, depth: usize, parent_length: usize) -> Vec<u8> {
        let mut node = self.nodes.remove(&num_node).unwrap();

        // Write all child offsets inside the current node
        for child_num in node.childs.iter() {
            let current_length = self.calculate_length(depth, node.data.len(), parent_length);
            let child = self.nodes.get(child_num).unwrap().clone();

            match child.node_type {
                DataNodeType::Array => {
                    let mut child_data = self.assemble_node(*child_num, depth + 1, current_length);
                    // Count
                    let count = child.element_offsets.len();
                    LittleEndian::write_u16(
                        &mut node.data[child.parent_offset..child.parent_offset + 2],
                        count as u16,
                    );
                    // First element offset
                    LittleEndian::write_u16(
                        &mut node.data[child.parent_offset + 2..child.parent_offset + 4],
                        current_length as u16,
                    );
                    node.data.append(&mut child_data);
                }
                DataNodeType::Bytes => {
                    let mut child_data = self.assemble_node(*child_num, depth, current_length);
                    // Offset
                    LittleEndian::write_u16(
                        &mut node.data[child.parent_offset..child.parent_offset + 2],
                        current_length as u16,
                    );
                    // Data length
                    let data_length = child_data.len();
                    LittleEndian::write_u16(
                        &mut node.data[child.parent_offset + 2..child.parent_offset + 4],
                        data_length as u16,
                    );
                    node.data.append(&mut child_data);
                }
                DataNodeType::String => {
                    let mut child_data = self.assemble_node(*child_num, depth, current_length);
                    // Offset
                    LittleEndian::write_u16(
                        &mut node.data[child.parent_offset..child.parent_offset + 2],
                        current_length as u16,
                    );
                    node.data.append(&mut child_data);
                }
                DataNodeType::Root => {
                    panic!("Found root data node in child, this should not happen!");
                }
            }

            // Write all elements offsets of a child (linked list elements)
            let count = child.element_offsets.len();
            for i in 0..count {
                // Current element offset
                let current_element_offset = child.element_offsets.get(i).unwrap();
                let offset = current_element_offset + current_length;
                LittleEndian::write_u16(
                    &mut node.data[child.parent_offset + 2..child.parent_offset + 4],
                    offset as u16,
                );
                // Next element offset
                if i + 1 < count {
                    let next_element_offset = child.element_offsets.get(i).unwrap();
                    let offset = next_element_offset + current_length;
                    LittleEndian::write_u16(
                        &mut node.data[child.parent_offset + 2..child.parent_offset + 4],
                        offset as u16,
                    );
                } else {
                    LittleEndian::write_u16(
                        &mut node.data[child.parent_offset + 2..child.parent_offset + 4],
                        0x0 as u16,
                    );
                }
            }
        }
        node.data
    }
}

/// Serializes the given structure into a `Vec<u8>` byte stream for the TERA network protocol.
pub fn to_vec<T>(value: T) -> Result<Vec<u8>>
where
    T: Serialize,
{
    let root_node = DataNode {
        node_type: DataNodeType::Root,
        parent: 0,
        childs: Vec::with_capacity(0),
        element_offsets: Vec::with_capacity(0),
        // TODO benchmark me
        data: Vec::with_capacity(1024),
        parent_offset: 0,
    };

    let mut serializer = Serializer {
        current_node: 0,
        nodes: HashMap::new(),
    };
    serializer.nodes.insert(0, root_node);
    value.serialize(&mut serializer)?;

    // Recursively assemble the data
    Ok(serializer.assemble_node(0, 0, 4)) // 4 bytes packet header
}

macro_rules! impl_nums {
    ($ty:ty, $ser_method:ident, $writer_method:ident, $value_size:literal) => {
        #[inline]
        fn $ser_method(self, value: $ty) -> Result<()> {
            let mut buf = vec![0; $value_size];
            LittleEndian::$writer_method(&mut buf, value);
            self.nodes
                .get_mut(&self.current_node)
                .unwrap()
                .data
                .append(&mut buf);
            Ok(())
        }
    }
}

impl<'a> ser::Serializer for &'a mut Serializer {
    type Ok = ();
    type Error = Error;

    type SerializeSeq = Self;
    type SerializeTuple = Self;
    type SerializeTupleStruct = Self;
    type SerializeTupleVariant = Self;
    type SerializeMap = Self;
    type SerializeStruct = Self;
    type SerializeStructVariant = Self;

    fn serialize_unit(self) -> Result<()> {
        Ok(())
    }

    fn serialize_bool(self, value: bool) -> Result<()> {
        let val: u8 = if value { 0x1 } else { 0x0 };
        self.nodes
            .get_mut(&self.current_node)
            .unwrap()
            .data
            .push(val);
        Ok(())
    }

    fn serialize_u8(self, value: u8) -> Result<()> {
        self.nodes
            .get_mut(&self.current_node)
            .unwrap()
            .data
            .push(value);
        Ok(())
    }

    fn serialize_i8(self, value: i8) -> Result<()> {
        self.nodes
            .get_mut(&self.current_node)
            .unwrap()
            .data
            .push(value as u8);
        Ok(())
    }

    impl_nums!(u16, serialize_u16, write_u16, 2);
    impl_nums!(u32, serialize_u32, write_u32, 4);
    impl_nums!(u64, serialize_u64, write_u64, 8);
    impl_nums!(i16, serialize_i16, write_i16, 2);
    impl_nums!(i32, serialize_i32, write_i32, 4);
    impl_nums!(i64, serialize_i64, write_i64, 8);
    impl_nums!(f32, serialize_f32, write_f32, 4);
    impl_nums!(f64, serialize_f64, write_f64, 8);

    fn serialize_char(self, _value: char) -> Result<()> {
        Err(Error::NotImplemented())
    }

    fn serialize_str(self, value: &str) -> Result<()> {
        let num_node = self.nodes.len();
        let nodes = &mut self.nodes;

        let parent_node = nodes.get_mut(&self.current_node).unwrap();
        parent_node.childs.push(num_node);

        let parent_data = parent_node.data.as_mut_slice();

        // Convert UTF-8 to UCS2
        let mut aligned = vec![0; value.len() * 3];
        let len = ucs2::encode(value, aligned.as_mut_slice()).unwrap();
        let mut buffer = vec![0; len * 2];
        LittleEndian::write_u16_into(&aligned[..len], &mut buffer);
        // End with null termination
        buffer.push(0x00);
        buffer.push(0x00);

        // Add new data node, link parent and register as child in parent.
        let mut new_node = DataNode {
            node_type: DataNodeType::String,
            parent: self.current_node,
            childs: Vec::new(),
            element_offsets: Vec::with_capacity(0),
            data: Vec::with_capacity(len + 2), // +2 = null termination
            parent_offset: parent_data.len(),
        };

        // Write u16 offset as dummy in parent data buffer
        parent_node.data.push(0xfe);
        parent_node.data.push(0xfe);

        new_node.data.append(&mut buffer.to_vec());
        self.nodes.insert(num_node, new_node);

        Ok(())
    }

    fn serialize_bytes(self, value: &[u8]) -> Result<()> {
        // Save current pos in stream as ABS_POS
        // Write offset as dummy and number of bytes
        // Add new data node and link parent and register as child in parent.
        // Fill data node with the data
        Ok(())
    }

    fn serialize_none(self) -> Result<()> {
        Err(Error::NotImplemented())
    }

    fn serialize_some<T>(self, _value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        Err(Error::NotImplemented())
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<()> {
        Err(Error::NotImplemented())
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
    ) -> Result<()> {
        Ok(())
    }

    fn serialize_newtype_struct<T>(self, _name: &'static str, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T>(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _value: &T,
    ) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        Ok(())
    }

    fn serialize_seq(self, _len: Option<usize>) -> Result<Self::SerializeSeq> {
        // TODO can we use the len here?
        // Write down count.
        // Safe the current ABS_POS, and safe a dummy offset.
        // Add new data node and link parent and register as child in parent.
        // Also safe the ABS_POS in it.
        Ok(self)
    }

    fn serialize_tuple(self, _len: usize) -> Result<Self::SerializeTuple> {
        Ok(self)
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleStruct> {
        Ok(self)
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleVariant> {
        Err(Error::NotImplemented())
    }

    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap> {
        Err(Error::NotImplemented())
    }

    fn serialize_struct(self, _name: &'static str, _len: usize) -> Result<Self::SerializeStruct> {
        Ok(self)
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStructVariant> {
        Err(Error::NotImplemented())
    }
}

impl<'a> ser::SerializeSeq for &'a mut Serializer {
    type Ok = ();
    type Error = Error;

    fn serialize_element<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<()> {
        let parent = self.nodes.get(&self.current_node).unwrap().parent;
        self.current_node = parent;
        Ok(())
    }
}

impl<'a> ser::SerializeTuple for &'a mut Serializer {
    type Ok = ();
    type Error = Error;

    fn serialize_element<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<()> {
        Ok(())
    }
}

impl<'a> ser::SerializeTupleStruct for &'a mut Serializer {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<()> {
        Ok(())
    }
}

impl<'a> ser::SerializeTupleVariant for &'a mut Serializer {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<()> {
        Ok(())
    }
}

impl<'a> ser::SerializeMap for &'a mut Serializer {
    type Ok = ();
    type Error = Error;

    fn serialize_key<T>(&mut self, _key: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        Err(Error::NotImplemented())
    }

    fn serialize_value<T>(&mut self, _value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        Err(Error::NotImplemented())
    }

    fn end(self) -> Result<()> {
        Err(Error::NotImplemented())
    }
}

impl<'a> ser::SerializeStruct for &'a mut Serializer {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T>(&mut self, _key: &'static str, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<()> {
        Ok(())
    }
}

impl<'a> ser::SerializeStructVariant for &'a mut Serializer {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T>(&mut self, _key: &'static str, _value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        Err(Error::NotImplemented())
    }

    fn end(self) -> Result<()> {
        Err(Error::NotImplemented())
    }
}

// The serializer and deserializer are tested in the packet definition with real world data.
#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;

    #[test]
    fn test_primitive_struct() {
        #[derive(Serialize, PartialEq, Debug)]
        struct SimpleStruct {
            a: u8,
            b: i8,
            c: f32,
            d: f64,
        }

        let data = SimpleStruct {
            a: 18,
            b: -13,
            c: 2.2,
            d: 1.0,
        };
        let expected = vec![
            0x12, 0xf3, 0xCD, 0xCC, 0x0C, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xf0, 0x3f,
        ];

        assert_eq!(expected, to_vec(data).unwrap());
    }
}
