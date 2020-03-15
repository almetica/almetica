use std::collections::HashMap;

use byteorder::{ByteOrder, LittleEndian};
use serde::{ser, Serialize};
use super::{Error, Result};

#[derive(Debug, Clone)]
pub struct Serializer {
    current_node: usize,
    nodes: HashMap<usize, DataNode>,
}

#[derive(Debug, Clone)]
struct DataNode {
    parent: usize,
    childs: Vec<usize>,
    data: Vec<u8>,
    parent_offset: usize,
}

impl Serializer {

    // Recursively assemble to data nodes into one packet
    fn assemble_node(&mut self, num_node: usize) -> Vec<u8> {
        let mut node = self.nodes.remove(&num_node).unwrap();
        for child_num in node.childs.iter() {
            
            // Write data offset in proper parent data location
            let child = self.nodes.get(child_num).unwrap();
            let num = node.data.len();
            LittleEndian::write_u16(&mut node.data[child.parent_offset..child.parent_offset+2], (num + 4) as u16);

            // TODO do we write the second offset in an array?

            let mut child_data = self.assemble_node(*child_num);
            node.data.append(&mut child_data);
        }
        node.data
    }
}

/// Serializes the given structure into a `Vec<u8>` byte stream for the Tera network protocol.
pub fn to_vec<T>(value: T) -> Result<Vec<u8>>
where
    T: Serialize,
{
    let root_node = DataNode {
        parent: 0,
        childs: Vec::with_capacity(0),
        // TODO benchmark me
        data: Vec::with_capacity(4096),
        parent_offset: 0,
    };

    let mut serializer = Serializer {
        current_node: 0,
        nodes: HashMap::new(),
    };
    serializer.nodes.insert(0, root_node);
    
    value.serialize(&mut serializer)?;

    // Recursively assemble the data
    Ok(serializer.assemble_node(0))
}

macro_rules! impl_nums {
    ($ty:ty, $ser_method:ident, $writer_method:ident, $value_size:literal) => {
        #[inline]
        fn $ser_method(self, value: $ty) -> Result<()> {
            let mut buf = vec![0; $value_size];
            LittleEndian::$writer_method(&mut buf, value);
            self.nodes.get_mut(&self.current_node).unwrap().data.append(&mut buf);
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

    fn serialize_bool(self, v: bool) -> Result<()> {
        let val: u8 = if v { 0x1 } else { 0x0 };
        self.nodes.get_mut(&self.current_node).unwrap().data.push(val);
        Ok(())
    }

    fn serialize_u8(self, v: u8) -> Result<()> {
        self.nodes.get_mut(&self.current_node).unwrap().data.push(v);
        Ok(())
    }

    fn serialize_i8(self, v: i8) -> Result<()> {
        // TODO test me
        self.nodes.get_mut(&self.current_node).unwrap().data.push(v as u8);
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

    fn serialize_char(self, _v: char) -> Result<()> {
        Err(Error::NotImplemented())
    }

    fn serialize_str(self, v: &str) -> Result<()> {
        // The beauty of Rust... HashMap are missing the IndexMut trait...
        let num_node = self.nodes.len();
        let nodes = &mut self.nodes;

        let parent_node = nodes.get_mut(&self.current_node).unwrap(); 
        parent_node.childs.push(num_node);

        let parent_data = parent_node.data.as_mut_slice();

        // Convert UTF-8 to UCS2
        let mut aligned = vec![0; v.len() * 3];
        let len = ucs2::encode(v, aligned.as_mut_slice()).unwrap();
        let mut buffer = vec![0; len * 2];
        LittleEndian::write_u16_into(&aligned[..len], &mut buffer);
        // End with null termination
        buffer.push(0x00);
        buffer.push(0x00);

        // Add new data node, link parent and register as child in parent.
        let mut new_node = DataNode {
            parent: self.current_node,
            childs: Vec::new(),
            data: Vec::with_capacity(len+2), // +2 = null termination
            parent_offset: parent_data.len(),
        };

        // Write u16 offset as dummy in parent data buffer
        parent_node.data.push(0xfe);
        parent_node.data.push(0xfe);

        new_node.data.append(&mut buffer.to_vec());
        self.nodes.insert(num_node, new_node);

        Ok(())
    }

    fn serialize_bytes(self, _value: &[u8]) -> Result<()> {
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

    fn serialize_newtype_struct<T>(
        self,
        _name: &'static str,
        value: &T,
    ) -> Result<()>
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

    fn serialize_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStruct> {
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