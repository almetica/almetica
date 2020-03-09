use std::str;

use super::error::{Error, Result};
use byteorder::{ByteOrder, LittleEndian};
use serde::de::IntoDeserializer;
use serde::{self, Deserialize};

/// A Deserializer that reads bytes from a vector.
#[derive(Clone, Debug)]
pub struct Deserializer {
    data: Vec<u8>,
    pos: usize,
}

/// Parses the given `Vec<u8>`
pub fn from_vec<'a, T>(v: Vec<u8>) -> Result<T>
where
    T: Deserialize<'a>,
{
    let mut deserializer = Deserializer::from_vec(v);
    let t = T::deserialize(&mut deserializer)?;
    // FIXME: We currently don't test if we have read the whole message
    Ok(t)
}

impl<'de> Deserializer {
    /// Creates a new Deserializer with a given `Vec<u8>`.
    pub fn from_vec(r: Vec<u8>) -> Self {
        Deserializer { data: r, pos: 0 }
    }
}

macro_rules! impl_nums {
    ($ty:ty, $dser_method:ident, $visitor_method:ident, $reader_method:ident, $size:literal) => {
        #[inline]
        fn $dser_method<V>(self, visitor: V) -> Result<V::Value>
            where V: serde::de::Visitor<'de>,
        {
            let d = LittleEndian::$reader_method(&self.data[self.pos..self.pos + $size]);
            self.pos += $size;
            visitor.$visitor_method(d)
        }
    }
}

impl<'de, 'a> serde::Deserializer<'de> for &'a mut Deserializer {
    type Error = Error;

    #[inline]
    fn deserialize_any<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: serde::de::Visitor<'de>,
    {
        Err(Error::DeserializeAnyNotSupported(self.pos))
    }

    #[inline]
    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value>
    where
        V: serde::de::Visitor<'de>,
    {
        let pos = self.pos;
        let value: u8 = serde::Deserialize::deserialize(self)?;
        match value {
            1 => visitor.visit_bool(true),
            0 => visitor.visit_bool(false),
            v => Err(Error::InvalidBoolEncoding(v, pos)),
        }
    }

    #[inline]
    fn deserialize_u8<V>(self, visitor: V) -> Result<V::Value>
    where
        V: serde::de::Visitor<'de>,
    {
        self.pos += 1;
        visitor.visit_u8(self.data[self.pos - 1])
    }

    #[inline]
    fn deserialize_i8<V>(self, visitor: V) -> Result<V::Value>
    where
        V: serde::de::Visitor<'de>,
    {
        self.pos += 1;
        visitor.visit_i8(self.data[self.pos - 1] as i8)
    }

    impl_nums!(u16, deserialize_u16, visit_u16, read_u16, 2);
    impl_nums!(u32, deserialize_u32, visit_u32, read_u32, 4);
    impl_nums!(u64, deserialize_u64, visit_u64, read_u64, 8);
    impl_nums!(i16, deserialize_i16, visit_i16, read_i16, 2);
    impl_nums!(i32, deserialize_i32, visit_i32, read_i32, 4);
    impl_nums!(i64, deserialize_i64, visit_i64, read_i64, 8);
    impl_nums!(f32, deserialize_f32, visit_f32, read_f32, 4);
    impl_nums!(f64, deserialize_f64, visit_f64, read_f64, 8);

    #[inline]
    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_unit()
    }

    // TODO: Maybe we shouldn't support this at all!
    #[inline]
    fn deserialize_char<V>(self, visitor: V) -> Result<V::Value>
    where
        V: serde::de::Visitor<'de>,
    {
        Err(Error::DeserializeCharNotSupported(self.pos))
    }

    // TODO refactor me
    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value>
    where
        V: serde::de::Visitor<'de>,
    {
        let rel_offset: u16 = serde::Deserialize::deserialize(&mut *self)?;
        let abs_offset = self.pos - 2 + rel_offset as usize;
        for i in (abs_offset..self.data.len()).step_by(2) {
            // Look for null terminator
            if self.data[i] == 0 && self.data[i + 1] == 0 {
                let mut aligned = vec![0u16; i / 2];
                for j in 0..aligned.len() {
                    aligned[i] = LittleEndian::read_u16(
                        &self.data[abs_offset + j..abs_offset as usize + j + 2],
                    );
                }
                let mut utf8 = vec![0u8; aligned.len() * 3];
                let size = ucs2::decode(&aligned, &mut utf8).unwrap();
                let s: &str;

                unsafe {
                    s = str::from_utf8_unchecked(&utf8[..size]);
                }

                return visitor.visit_str(s);
            }
        }
        Err(Error::StringNotNullTerminated(self.pos))
    }

    // TODO refactor me
    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value>
    where
        V: serde::de::Visitor<'de>,
    {
        // TODO Is this actually the absolute value?
        let rel_offset: u16 = serde::Deserialize::deserialize(&mut *self)?;
        let abs_offset = self.pos - 2 + rel_offset as usize;
        println!("abs_offset = {}", abs_offset);
        for i in (abs_offset..self.data.len()).step_by(2) {
            // Look for null terminator
            if self.data[i] == 0 && self.data[i + 1] == 0 {
                let mut aligned = vec![0u16; i / 2];
                for j in 0..aligned.len() {
                    aligned[i] = LittleEndian::read_u16(
                        &self.data[abs_offset + j..abs_offset as usize + j + 2],
                    );
                }
                let mut utf8 = vec![0u8; aligned.len() * 3];
                let size = ucs2::decode(&aligned, &mut utf8).unwrap();
                let s: &str;

                unsafe {
                    s = str::from_utf8_unchecked(&utf8[..size]);
                }

                return visitor.visit_string(s.to_string());
            }
        }
        Err(Error::StringNotNullTerminated(self.pos))
    }

    // TODO refactor me
    fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value>
    where
        V: serde::de::Visitor<'de>,
    {
        let len: u16 = serde::Deserialize::deserialize(&mut *self)?;
        let offset: u16 = serde::Deserialize::deserialize(&mut *self)?;
        if (self.pos as u16 - 4 + len + offset) as usize > self.data.len() {
            return Err(Error::BytesTooBig(self.pos));
        };
        visitor.visit_bytes(&self.data[(offset - 4) as usize..(offset - 4 + len) as usize])
    }

    // TODO refactor me
    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value>
    where
        V: serde::de::Visitor<'de>,
    {
        let len: u16 = serde::Deserialize::deserialize(&mut *self)?;
        let offset: u16 = serde::Deserialize::deserialize(&mut *self)?;
        if (self.pos as u16 - 4 + len + offset) as usize > self.data.len() {
            return Err(Error::BytesTooBig(self.pos));
        };
        let s = &self.data[(offset - 4) as usize..((offset - 4) + len) as usize];
        visitor.visit_byte_buf(s.to_vec())
    }

    fn deserialize_enum<V>(
        self,
        _enum: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value>
    where
        V: serde::de::Visitor<'de>,
    {
        impl<'de, 'a> serde::de::EnumAccess<'de> for &'a mut Deserializer {
            type Error = Error;
            type Variant = Self;

            fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant)>
            where
                V: serde::de::DeserializeSeed<'de>,
            {
                // TODO enums might need different sizes. So we might need to use attributes here?
                let idx: u32 = serde::de::Deserialize::deserialize(&mut *self)?;
                let val: Result<_> = seed.deserialize(idx.into_deserializer());
                Ok((val?, self))
            }
        }

        visitor.visit_enum(self)
    }

    // TODO Test me! This is also used for structs and vecs!
    fn deserialize_tuple<V>(self, count: usize, visitor: V) -> Result<V::Value>
    where
        V: serde::de::Visitor<'de>,
    {
        struct Access<'a> {
            deserializer: &'a mut Deserializer,
            count: usize,
        }

        impl<'de, 'a, 'b: 'a> serde::de::SeqAccess<'de> for Access<'a> {
            type Error = Error;

            fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>>
            where
                T: serde::de::DeserializeSeed<'de>,
            {
                if self.count > 0 {
                    self.count -= 1;
                    let value =
                        serde::de::DeserializeSeed::deserialize(seed, &mut *self.deserializer)?;
                    Ok(Some(value))
                } else {
                    Ok(None)
                }
            }

            fn size_hint(&self) -> Option<usize> {
                Some(self.count)
            }
        }

        visitor.visit_seq(Access {
            deserializer: self,
            count: count,
        })
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value>
    where
        V: serde::de::Visitor<'de>,
    {
        Err(Error::DeserializeOptionNotSupported(self.pos))
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value>
    where
        V: serde::de::Visitor<'de>,
    {
        struct Access<'a> {
            deserializer: &'a mut Deserializer,
            count: usize,
            next_offset: u16,
            old_pos: usize,
        }

        impl<'de, 'a, 'b: 'a> serde::de::SeqAccess<'de> for Access<'a> {
            type Error = Error;

            fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>>
            where
                T: serde::de::DeserializeSeed<'de>,
            {
                if self.count > 0 {
                    self.count -= 1;

                    // The array is a linked list
                    self.deserializer.pos = (self.next_offset - 4u16) as usize;

                    let this_offset: u16 =
                        serde::Deserialize::deserialize(&mut *self.deserializer)?;
                    if this_offset != self.next_offset {
                        return Err(Error::InvalidSeqEntry((this_offset - 4u16) as usize));
                    }
                    self.next_offset = serde::Deserialize::deserialize(&mut *self.deserializer)?;
                    let value =
                        serde::de::DeserializeSeed::deserialize(seed, &mut *self.deserializer)?;
                    Ok(Some(value))
                } else {
                    // Return to the end of the array header
                    self.deserializer.pos = self.old_pos;
                    Ok(None)
                }
            }

            fn size_hint(&self) -> Option<usize> {
                Some(self.count)
            }
        }

        let count: u16 = serde::Deserialize::deserialize(&mut *self)?;
        let next_offset: u16 = serde::Deserialize::deserialize(&mut *self)?;
        let old_pos = self.pos.clone();

        visitor.visit_seq(Access {
            deserializer: self,
            count: count as usize,
            next_offset: next_offset,
            old_pos: old_pos,
        })
    }

    fn deserialize_map<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: serde::de::Visitor<'de>,
    {
        Err(Error::DeserializeMapNotSupported(self.pos))
    }

    fn deserialize_struct<V>(
        self,
        _name: &str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_tuple(fields.len(), visitor)
    }

    fn deserialize_identifier<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: serde::de::Visitor<'de>,
    {
        Err(Error::DeserializeIdentifierNotSupported(self.pos))
    }

    fn deserialize_newtype_struct<V>(self, _name: &str, visitor: V) -> Result<V::Value>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_unit_struct<V>(self, _name: &'static str, visitor: V) -> Result<V::Value>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_unit()
    }

    fn deserialize_tuple_struct<V>(
        self,
        _name: &'static str,
        len: usize,
        visitor: V,
    ) -> Result<V::Value>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_tuple(len, visitor)
    }

    fn deserialize_ignored_any<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: serde::de::Visitor<'de>,
    {
        Err(Error::DeserializeIgnoredAnyNotSupported(self.pos))
    }
}

impl<'de, 'a> serde::de::VariantAccess<'de> for &'a mut Deserializer {
    type Error = Error;

    fn unit_variant(self) -> Result<()> {
        Ok(())
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value>
    where
        T: serde::de::DeserializeSeed<'de>,
    {
        serde::de::DeserializeSeed::deserialize(seed, self)
    }

    fn tuple_variant<V>(self, len: usize, visitor: V) -> Result<V::Value>
    where
        V: serde::de::Visitor<'de>,
    {
        serde::de::Deserializer::deserialize_tuple(self, len, visitor)
    }

    fn struct_variant<V>(self, fields: &'static [&'static str], visitor: V) -> Result<V::Value>
    where
        V: serde::de::Visitor<'de>,
    {
        serde::de::Deserializer::deserialize_tuple(self, fields.len(), visitor)
    }
}

#[test]
fn test_primitive_struct() {
    #[derive(Deserialize, PartialEq, Debug)]
    struct Test {
        a: u8,
        b: i8,
        c: f32,
        d: f64,
    }

    let data = vec![
        0x12, 0xf3, 0xCD, 0xCC, 0x0C, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xf0, 0x3f,
    ];
    let expected = Test {
        a: 18,
        b: -13,
        c: 2.2,
        d: 1.0,
    };

    assert_eq!(expected, from_vec(data).unwrap());
}

#[test]
fn test_c_get_user_guild_logo() {
    #[derive(Deserialize, PartialEq, Debug)]
    struct Test {
        playerid: i32,
        guildid: i32,
    }

    let data = vec![0x1, 0x2f, 0x31, 0x1, 0x75, 0xe, 0x0, 0x0];
    let expected = Test {
        playerid: 20000513,
        guildid: 3701,
    };

    assert_eq!(expected, from_vec(data).unwrap());
}

#[test]
fn test_s_account_package_list() {
    #[derive(Deserialize, PartialEq, Debug)]
    struct AccountBenefits {
        package_id: u32,
        expiration_date: i64,
    }

    #[derive(Deserialize, PartialEq, Debug)]
    struct AcountPackageList {
        account_benefits: Vec<AccountBenefits>,
    }

    let data = vec![
        0x1, 0x0, 0x8, 0x0, 0x8, 0x0, 0x0, 0x0, 0xb2, 0x1, 0x0, 0x0, 0xff, 0xff, 0xff, 0x7f, 0x0,
        0x0, 0x0, 0x0,
    ];
    let expected = AcountPackageList {
        account_benefits: vec![AccountBenefits {
            package_id: 434,
            expiration_date: 2147483647,
        }],
    };

    assert_eq!(expected, from_vec(data).unwrap());
}

#[test]
fn test_s_item_custom_string() {
    #[derive(Deserialize, PartialEq, Debug)]
    struct ItemCustomString {
        custom_strings: Vec<CustomStrings>,
        game_id: u64,
    }

    #[derive(Deserialize, PartialEq, Debug)]
    struct CustomStrings {
        string: String,
        db_id: u64,
    }

    let mut data = vec![
        0x0, 0x0, 0x0, 0x0, 0x11, 0x7f, 0x1c, 0x0, 0x0, 0x80, 0x0, 0x2,
    ];
    let mut expected = ItemCustomString {
        custom_strings: Vec::with_capacity(0),
        game_id: 144255925566078737,
    };
    assert_eq!(expected, from_vec(data).unwrap());

    data = vec![
        0x1, 0x0, 0x10, 0x0, 0x4f, 0x3, 0x1c, 0x0, 0x0, 0x80, 0x0, 0x3, 0x10, 0x0, 0x0, 0x0, 0x1a,
        0x0, 0x61, 0xb6, 0x2, 0x0, 0x50, 0x0, 0x61, 0x0, 0x6e, 0x0, 0x74, 0x0, 0x73, 0x0, 0x75,
        0x0, 0x0, 0x0,
    ];
    expected = ItemCustomString {
        custom_strings: vec![CustomStrings {
            string: "Pantsu".to_string(),
            db_id: 763477683208192,
        }],
        game_id: 144255925566078737,
    };
    assert_eq!(expected, from_vec(data).unwrap());
}
