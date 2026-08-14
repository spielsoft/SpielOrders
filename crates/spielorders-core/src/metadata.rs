//! Typed codecs for SpielOrders-managed metadata.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Version1;

impl Serialize for Version1 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_u8(1)
    }
}

macro_rules! uuid_v4_id {
    ($name:ident) => {
        #[derive(Clone, Debug, Eq, PartialEq)]
        pub struct $name(String);

        impl $name {
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                serializer.serialize_str(&self.0)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                let text = String::deserialize(deserializer)?;
                if is_lowercase_uuid_v4(&text) {
                    Ok(Self(text))
                } else {
                    Err(serde::de::Error::custom(
                        "expected a lowercase hyphenated UUIDv4",
                    ))
                }
            }
        }
    };
}

uuid_v4_id!(OrderId);
uuid_v4_id!(VendorId);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageRoot(String);

impl PackageRoot {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Serialize for PackageRoot {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for PackageRoot {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let text = String::deserialize(deserializer)?;
        if text.split('/').count() >= 2 && text.split('/').all(|segment| segment == "..") {
            Ok(Self(text))
        } else {
            Err(serde::de::Error::custom(
                "expected two or more parent-only path segments",
            ))
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NonBlankString(String);

impl NonBlankString {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn normalize_single_line(&mut self) {
        self.0 = self.0.trim().to_owned();
    }
}

pub(crate) fn normalize_optional_single_line(value: &mut Option<String>) {
    *value = value
        .take()
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty());
}

pub(crate) fn normalize_optional_multiline(value: &mut Option<String>) {
    *value = value
        .take()
        .map(|text| text.replace("\r\n", "\n").replace('\r', "\n"))
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty());
}

impl Serialize for NonBlankString {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for NonBlankString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let text = String::deserialize(deserializer)?;
        if text.chars().any(|character| !character.is_whitespace()) {
            Ok(Self(text))
        } else {
            Err(serde::de::Error::custom("expected nonblank text"))
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DateOrdered(String);

impl DateOrdered {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Serialize for DateOrdered {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for DateOrdered {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let text = String::deserialize(deserializer)?;
        if is_date_shape(&text) {
            Ok(Self(text))
        } else {
            Err(serde::de::Error::custom(
                "expected a YYYY-MM-DD date with structural month and day ranges",
            ))
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Currency(String);

impl Currency {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Serialize for Currency {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Currency {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let text = String::deserialize(deserializer)?;
        if text.len() == 3 && text.bytes().all(|byte| byte.is_ascii_uppercase()) {
            Ok(Self(text))
        } else {
            Err(serde::de::Error::custom(
                "expected three uppercase ASCII letters",
            ))
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexOrderDirectory(String);

impl IndexOrderDirectory {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Serialize for IndexOrderDirectory {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for IndexOrderDirectory {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let text = String::deserialize(deserializer)?;
        let valid = !text.is_empty()
            && !text.starts_with('/')
            && !matches!(text.as_bytes(), [drive, b':', ..] if drive.is_ascii_alphabetic())
            && !text.contains('\\')
            && text
                .split('/')
                .all(|segment| !segment.is_empty() && segment != "." && segment != "..");
        if valid {
            Ok(Self(text))
        } else {
            Err(serde::de::Error::custom(
                "expected a normalized relative slash-separated path",
            ))
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FiniteNumber(f64);

impl FiniteNumber {
    pub fn new(value: f64) -> Option<Self> {
        value.is_finite().then_some(Self(value))
    }

    pub fn get(self) -> f64 {
        self.0
    }
}

impl Serialize for FiniteNumber {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_f64(self.0)
    }
}

impl<'de> Deserialize<'de> for FiniteNumber {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = f64::deserialize(deserializer)?;
        Self::new(value).ok_or_else(|| serde::de::Error::custom("expected a finite number"))
    }
}

impl<'de> Deserialize<'de> for Version1 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let version = u64::deserialize(deserializer)?;
        if version == 1 {
            Ok(Self)
        } else {
            Err(serde::de::Error::custom("expected metadata version 1"))
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default = "default_orders_path")]
    pub orders_path: String,
    #[serde(default = "default_removed_path")]
    pub removed_path: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VersionDocument {
    pub metadata_version: Version1,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VendorsDocument {
    pub schema_version: Version1,
    pub vendors: Vec<Vendor>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Vendor {
    pub vendor_id: VendorId,
    pub name: NonBlankString,
    #[serde(
        default,
        deserialize_with = "deserialize_present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub phone: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub email: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub website: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub address: Option<String>,
    pub contacts: Vec<VendorContact>,
    #[serde(
        default,
        deserialize_with = "deserialize_present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub purchasing_notes: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VendorContact {
    pub name: NonBlankString,
    #[serde(
        default,
        deserialize_with = "deserialize_present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub role: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub phone: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub email: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IndexDocument {
    pub schema_version: Version1,
    pub orders: Vec<IndexOrder>,
    pub vendors: Vec<Vendor>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IndexOrder {
    pub order_directory: IndexOrderDirectory,
    pub order: OrderDocument,
    pub readme: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OrderDocument {
    pub schema_version: Version1,
    pub package_root: PackageRoot,
    pub order_id: OrderId,
    pub title: NonBlankString,
    pub date_ordered: DateOrdered,
    pub status: OrderStatus,
    #[serde(
        default,
        deserialize_with = "deserialize_present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub vendor_id: Option<VendorId>,
    #[serde(
        default,
        deserialize_with = "deserialize_present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub orderer: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub ordering_institution: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub currency: Option<Currency>,
    #[serde(
        default,
        deserialize_with = "deserialize_present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub project: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub order_group: Option<String>,
    pub items: Vec<OrderItem>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OrderItem {
    pub description: NonBlankString,
    pub quantity: FiniteNumber,
    pub unit_price: FiniteNumber,
    #[serde(
        default,
        deserialize_with = "deserialize_present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub catalog_number: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderStatus {
    InPreparation,
    Hold,
    Ordered,
    Received,
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MetadataErrorKind {
    Encoding,
    Syntax,
    Shape,
    Serialization,
}

#[derive(Debug)]
pub struct MetadataError {
    kind: MetadataErrorKind,
    message: String,
}

impl MetadataError {
    pub fn kind(&self) -> MetadataErrorKind {
        self.kind
    }
}

impl std::fmt::Display for MetadataError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for MetadataError {}

macro_rules! impl_json_document {
    ($document:ty) => {
        impl $document {
            pub fn from_json_bytes(bytes: &[u8]) -> Result<Self, MetadataError> {
                from_json_bytes(bytes)
            }

            pub fn to_json_bytes(&self) -> Result<Vec<u8>, MetadataError> {
                to_json_bytes(self)
            }
        }
    };
}

impl_json_document!(OrderDocument);
impl_json_document!(VersionDocument);
impl_json_document!(VendorsDocument);
impl_json_document!(IndexDocument);

impl Config {
    pub fn from_toml_bytes(bytes: &[u8]) -> Result<Self, MetadataError> {
        let text = std::str::from_utf8(bytes).map_err(|error| MetadataError {
            kind: MetadataErrorKind::Encoding,
            message: error.to_string(),
        })?;
        let table = text.parse::<toml::Table>().map_err(|error| MetadataError {
            kind: MetadataErrorKind::Syntax,
            message: error.to_string(),
        })?;
        toml::Value::Table(table)
            .try_into()
            .map_err(|error: toml::de::Error| MetadataError {
                kind: MetadataErrorKind::Shape,
                message: error.to_string(),
            })
    }
}

fn default_orders_path() -> String {
    "Orders".into()
}

fn default_removed_path() -> String {
    "Removed".into()
}

fn deserialize_present_optional<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
where
    T: Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    T::deserialize(deserializer).map(Some)
}

fn is_lowercase_uuid_v4(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 36
        && bytes[8] == b'-'
        && bytes[13] == b'-'
        && bytes[18] == b'-'
        && bytes[23] == b'-'
        && bytes[14] == b'4'
        && matches!(bytes[19], b'8' | b'9' | b'a' | b'b')
        && bytes.iter().enumerate().all(|(index, byte)| {
            matches!(index, 8 | 13 | 18 | 23)
                || byte.is_ascii_digit()
                || matches!(byte, b'a'..=b'f')
        })
}

fn is_date_shape(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
    {
        return false;
    }
    let month = (bytes[5] - b'0') * 10 + (bytes[6] - b'0');
    let day = (bytes[8] - b'0') * 10 + (bytes[9] - b'0');
    (1..=12).contains(&month) && (1..=31).contains(&day)
}

fn from_json_bytes<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, MetadataError> {
    if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        return Err(MetadataError {
            kind: MetadataErrorKind::Encoding,
            message: "managed JSON must not contain a byte-order mark".into(),
        });
    }
    let text = std::str::from_utf8(bytes).map_err(|error| MetadataError {
        kind: MetadataErrorKind::Encoding,
        message: error.to_string(),
    })?;
    serde_json::from_str(text).map_err(json_error)
}

fn to_json_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, MetadataError> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|error| MetadataError {
        kind: MetadataErrorKind::Serialization,
        message: error.to_string(),
    })?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn json_error(error: serde_json::Error) -> MetadataError {
    let kind = match error.classify() {
        serde_json::error::Category::Data => MetadataErrorKind::Shape,
        serde_json::error::Category::Io
        | serde_json::error::Category::Syntax
        | serde_json::error::Category::Eof => MetadataErrorKind::Syntax,
    };
    MetadataError {
        kind,
        message: error.to_string(),
    }
}
