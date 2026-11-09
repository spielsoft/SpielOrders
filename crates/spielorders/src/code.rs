//! The stable code a unit enum carries: the `snake_case` serde name of its variant, so the
//! spelling a caller branches on is derived from the type rather than kept in a table
//! beside it.

use serde::{
    Serialize, Serializer,
    ser::{self, Impossible},
};
use std::fmt::{self, Display};

/// A unit enum, such as an operation kind or a diagnostic code, whose code is the name its
/// variant serializes with.
pub trait Coded: Serialize {
    fn code(&self) -> &'static str {
        variant_name(self).expect("a coded value is a unit variant")
    }
}

/// The name a unit variant serializes with.
pub(crate) fn variant_name<T: Serialize + ?Sized>(value: &T) -> Option<&'static str> {
    value.serialize(VariantName).ok()
}

#[derive(Debug)]
struct NotAVariant;

impl Display for NotAVariant {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("not an enum variant")
    }
}

impl std::error::Error for NotAVariant {}

impl ser::Error for NotAVariant {
    fn custom<T: Display>(_message: T) -> Self {
        Self
    }
}

/// A serializer that answers only the name of the unit variant it is shown.
struct VariantName;

macro_rules! not_a_variant {
    ($($method:ident($($argument:ty),*);)*) => {
        $(fn $method(self, $(_: $argument),*) -> Result<Self::Ok, Self::Error> {
            Err(NotAVariant)
        })*
    };
}

impl Serializer for VariantName {
    type Ok = &'static str;
    type Error = NotAVariant;
    type SerializeSeq = Impossible<Self::Ok, Self::Error>;
    type SerializeTuple = Impossible<Self::Ok, Self::Error>;
    type SerializeTupleStruct = Impossible<Self::Ok, Self::Error>;
    type SerializeTupleVariant = Impossible<Self::Ok, Self::Error>;
    type SerializeMap = Impossible<Self::Ok, Self::Error>;
    type SerializeStruct = Impossible<Self::Ok, Self::Error>;
    type SerializeStructVariant = Impossible<Self::Ok, Self::Error>;

    not_a_variant! {
        serialize_bool(bool);
        serialize_i8(i8);
        serialize_i16(i16);
        serialize_i32(i32);
        serialize_i64(i64);
        serialize_u8(u8);
        serialize_u16(u16);
        serialize_u32(u32);
        serialize_u64(u64);
        serialize_f32(f32);
        serialize_f64(f64);
        serialize_char(char);
        serialize_str(&str);
        serialize_bytes(&[u8]);
        serialize_none();
        serialize_unit();
        serialize_unit_struct(&'static str);
    }

    fn serialize_some<T: Serialize + ?Sized>(self, _: &T) -> Result<Self::Ok, Self::Error> {
        Err(NotAVariant)
    }

    fn serialize_unit_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
    ) -> Result<Self::Ok, Self::Error> {
        Ok(variant)
    }

    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        _: &T,
    ) -> Result<Self::Ok, Self::Error> {
        Err(NotAVariant)
    }

    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: &T,
    ) -> Result<Self::Ok, Self::Error> {
        Err(NotAVariant)
    }

    fn serialize_seq(self, _: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        Err(NotAVariant)
    }

    fn serialize_tuple(self, _: usize) -> Result<Self::SerializeTuple, Self::Error> {
        Err(NotAVariant)
    }

    fn serialize_tuple_struct(
        self,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleStruct, Self::Error> {
        Err(NotAVariant)
    }

    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        Err(NotAVariant)
    }

    fn serialize_map(self, _: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        Err(NotAVariant)
    }

    fn serialize_struct(
        self,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStruct, Self::Error> {
        Err(NotAVariant)
    }

    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        Err(NotAVariant)
    }
}
