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
    fn deserialize_u16<V>(self, visitor: V) -> Result<V::Value>
    where
        V: serde::de::Visitor<'de>,
    {
        let d = LittleEndian::read_u16(&self.data[self.pos..self.pos + 2]);

        self.pos += 2;
        visitor.visit_u16(d)
    }

    #[inline]
    fn deserialize_u32<V>(self, visitor: V) -> Result<V::Value>
    where
        V: serde::de::Visitor<'de>,
    {
        let d = LittleEndian::read_u32(&self.data[self.pos..self.pos + 4]);

        self.pos += 4;
        visitor.visit_u32(d)
    }

    #[inline]
    fn deserialize_u64<V>(self, visitor: V) -> Result<V::Value>
    where
        V: serde::de::Visitor<'de>,
    {
        let d = LittleEndian::read_u32(&self.data[self.pos..self.pos + 8]);

        self.pos += 8;
        visitor.visit_u32(d)
    }

    // TOOD test me
    #[inline]
    fn deserialize_i8<V>(self, visitor: V) -> Result<V::Value>
    where
        V: serde::de::Visitor<'de>,
    {
        let i = self.data[self.pos] as i8;

        self.pos += 1;
        visitor.visit_i8(i)
    }

    #[inline]
    fn deserialize_i16<V>(self, visitor: V) -> Result<V::Value>
    where
        V: serde::de::Visitor<'de>,
    {
        let d = LittleEndian::read_i16(&self.data[self.pos..self.pos + 2]);

        self.pos += 2;
        visitor.visit_i16(d)
    }

    #[inline]
    fn deserialize_i32<V>(self, visitor: V) -> Result<V::Value>
    where
        V: serde::de::Visitor<'de>,
    {
        let d = LittleEndian::read_i32(&self.data[self.pos..self.pos + 4]);

        self.pos += 4;
        visitor.visit_i32(d)
    }

    #[inline]
    fn deserialize_i64<V>(self, visitor: V) -> Result<V::Value>
    where
        V: serde::de::Visitor<'de>,
    {
        let d = LittleEndian::read_i32(&self.data[self.pos..self.pos + 8]);

        self.pos += 8;
        visitor.visit_i32(d)
    }

    #[inline]
    fn deserialize_f32<V>(self, visitor: V) -> Result<V::Value>
    where
        V: serde::de::Visitor<'de>,
    {
        let d = LittleEndian::read_f32(&self.data[self.pos..self.pos + 4]);

        self.pos += 4;
        visitor.visit_f32(d)
    }

    #[inline]
    fn deserialize_f64<V>(self, visitor: V) -> Result<V::Value>
    where
        V: serde::de::Visitor<'de>,
    {
        let d = LittleEndian::read_f64(&self.data[self.pos..self.pos + 8]);

        self.pos += 8;
        visitor.visit_f64(d)
    }

    #[inline]
    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_unit()
    }

    #[inline]
    fn deserialize_char<V>(self, visitor: V) -> Result<V::Value>
    where
        V: serde::de::Visitor<'de>,
    {
        // We copy a 2 byte UCS2 char into a variable sized UTF-8 char
        let mut utf8 = [0u8; 3];
        let aligned = LittleEndian::read_u16(&self.data[self.pos..self.pos + 2]);
        let size = ucs2::decode(&[aligned], &mut utf8).unwrap();
        let res = str::from_utf8(&utf8[..size]).ok().unwrap();
        self.pos += 2;
        visitor.visit_char(res.chars().next().unwrap())
    }

    // TODO refactor me
    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value>
    where
        V: serde::de::Visitor<'de>,
    {
        let offset: u16 = serde::Deserialize::deserialize(&mut *self)?;

        for i in offset as usize..(offset as usize + self.data.len() - self.pos) {
            if self.data[i] == 0 {
                // If not, we don't have a UCS2 string.
                assert!(i % 2 == 0);

                let mut aligned = vec![0u16; i / 2];
                for j in 0..aligned.len() {
                    aligned[i] = LittleEndian::read_u16(
                        &self.data[offset as usize + j..offset as usize + j + 2],
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
        let offset: u16 = serde::Deserialize::deserialize(&mut *self)?;

        for i in offset as usize..(offset as usize + self.data.len() - self.pos) {
            if self.data[i] == 0 {
                // If not, we don't have a UCS2 string.
                assert!(i % 2 == 0);

                let mut aligned = vec![0u16; i / 2];
                for j in 0..aligned.len() {
                    aligned[i] = LittleEndian::read_u16(
                        &self.data[offset as usize + j..offset as usize + j + 2],
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
        if len + offset > std::u16::MAX {
            return Err(Error::BytesTooBig(self.pos));
        };
        visitor.visit_bytes(&self.data[offset as usize..(offset + len) as usize])
    }

    // TODO refactor me
    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value>
    where
        V: serde::de::Visitor<'de>,
    {
        let len: u16 = serde::Deserialize::deserialize(&mut *self)?;
        let offset: u16 = serde::Deserialize::deserialize(&mut *self)?;
        if len + offset > std::u16::MAX {
            return Err(Error::BytesTooBig(self.pos));
        };
        let s = &self.data[offset as usize..(offset + len) as usize];
        let v = s.to_vec();
        visitor.visit_byte_buf(v)
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
                // TODO enums might need different sizes. So we might need to use attributes here.
                let idx: u32 = serde::de::Deserialize::deserialize(&mut *self)?;
                let val: Result<_> = seed.deserialize(idx.into_deserializer());
                Ok((val?, self))
            }
        }

        visitor.visit_enum(self)
    }

    // TODO Test me! This is also used for structs!
    fn deserialize_tuple<V>(self, len: usize, visitor: V) -> Result<V::Value>
    where
        V: serde::de::Visitor<'de>,
    {
        struct Access<'a> {
            deserializer: &'a mut Deserializer,
            len: usize,
        }

        impl<'de, 'a, 'b: 'a> serde::de::SeqAccess<'de> for Access<'a> {
            type Error = Error;

            fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>>
            where
                T: serde::de::DeserializeSeed<'de>,
            {
                if self.len > 0 {
                    self.len -= 1;
                    let value =
                        serde::de::DeserializeSeed::deserialize(seed, &mut *self.deserializer)?;
                    Ok(Some(value))
                } else {
                    Ok(None)
                }
            }

            fn size_hint(&self) -> Option<usize> {
                Some(self.len)
            }
        }

        visitor.visit_seq(Access {
            deserializer: self,
            len: len,
        })
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value>
    where
        V: serde::de::Visitor<'de>,
    {
        let value: u8 = serde::de::Deserialize::deserialize(&mut *self)?;
        match value {
            0 => visitor.visit_none(),
            1 => visitor.visit_some(&mut *self),
            v => Err(Error::InvalidTagEncoding(v, self.pos)),
        }
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value>
    where
        V: serde::de::Visitor<'de>,
    {
        let len = serde::Deserialize::deserialize(&mut *self)?;
        self.deserialize_tuple(len, visitor)
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

    // This protocol is LE!
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

// TODO: Test more actual messages

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
