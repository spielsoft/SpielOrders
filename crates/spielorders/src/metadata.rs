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
        #[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
        pub struct $name(String);

        impl $name {
            pub(crate) fn as_str(&self) -> &str {
                &self.0
            }
        }

        text_deref!($name);

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

/// A validated text value, read as the text it validated.
macro_rules! text_deref {
    ($name:ident) => {
        impl std::ops::Deref for $name {
            type Target = str;

            fn deref(&self) -> &str {
                &self.0
            }
        }
    };
}

uuid_v4_id!(OrderId);
uuid_v4_id!(VendorId);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageRoot(String);

text_deref!(PackageRoot);

impl PackageRoot {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    /// The locator this text spells, or `None` where it spells none.
    ///
    /// The rule in one place, because it is asked twice: of a member being deserialized on
    /// its own, and of the member of an Order record, which carries the answer rather than
    /// failing.
    fn parse(text: &str) -> Option<Self> {
        (text.split('/').count() >= 2 && text.split('/').all(|segment| segment == ".."))
            .then(|| Self(text.to_owned()))
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
        Self::parse(&text).ok_or_else(|| {
            serde::de::Error::custom("expected two or more parent-only path segments")
        })
    }
}

/// A `package_root` member this build cannot read as a locator, kept exactly as written.
///
/// Opaque on purpose: nothing may interpret it — the decided repair recalculates the
/// locator from where the Order Directory physically sits. It is kept because a reader
/// is shown what the record actually stores, which is what tells a locator that leads
/// somewhere wrong from one that is not a locator at all.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct UninterpretableLocator(serde_json::Value);

impl UninterpretableLocator {
    /// The member's own text, where the member is text at all.
    pub(crate) fn as_text(&self) -> Option<&str> {
        self.0.as_str()
    }
}

/// One Order record's technical `package_root` locator, and whether this build can follow
/// it.
///
/// **The locator carries its own outcome rather than failing its record**, because a
/// damaged locator is the one Order-record damage the integrity decision calls repairable
/// by recalculation, and a record that failed to parse could only be diagnosed as an
/// invalid managed artifact. Every other rule the record has still applies through the
/// same derive. A consumer that needs the locator handles its absence rather than assuming
/// a record that loaded carries one.
///
/// A record with no usable locator is not a valid version-1 `order.json`, and the Package
/// load quarantines its Order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PackageRootLocator {
    /// Two or more parent-only segments: a locator this build can follow.
    Spelled(PackageRoot),
    /// No `package_root` member at all.
    Absent,
    /// A member that is not such a path.
    Malformed(UninterpretableLocator),
}

impl PackageRootLocator {
    /// The locator, where this record carries one this build can follow.
    pub(crate) fn spelled(&self) -> Option<&PackageRoot> {
        match self {
            Self::Spelled(root) => Some(root),
            Self::Absent | Self::Malformed(_) => None,
        }
    }

    /// The member's stored text, where the member is present and is text.
    ///
    /// What a reader is shown of a locator that led nowhere. An absent member and one
    /// that is not text have none to show, and the diagnosed reason is what says which.
    pub(crate) fn stored_text(&self) -> Option<&str> {
        match self {
            Self::Spelled(root) => Some(root.as_str()),
            Self::Absent => None,
            Self::Malformed(value) => value.as_text(),
        }
    }

    fn absent() -> Self {
        Self::Absent
    }

    fn is_absent(&self) -> bool {
        matches!(self, Self::Absent)
    }
}

impl Serialize for PackageRootLocator {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::Spelled(root) => root.serialize(serializer),
            Self::Malformed(value) => value.0.serialize(serializer),
            // Absent within a record is the member's absence, which the record's own
            // `skip_serializing_if` writes by omitting it. Alone it is a null.
            Self::Absent => serializer.serialize_none(),
        }
    }
}

impl<'de> Deserialize<'de> for PackageRootLocator {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        match value.as_str().and_then(PackageRoot::parse) {
            Some(root) => Ok(Self::Spelled(root)),
            None => Ok(Self::Malformed(UninterpretableLocator(value))),
        }
    }
}

/// One Order record as a derived Package Index carries it: healthy, locator and all.
///
/// The index holds only Orders the load judged healthy, and a healthy Order has a locator
/// this build can follow. A stored index naming one that does not is not this Package's
/// derived index, so it is refused here rather than trusted by the fast path that reads a
/// valid stored index instead of rebuilding it.
fn deserialize_indexed_order<'de, D>(deserializer: D) -> Result<OrderDocument, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let document = OrderDocument::deserialize(deserializer)?;
    if document.package_root.spelled().is_none() {
        return Err(serde::de::Error::custom(
            "expected an indexed Order to carry a usable package_root locator",
        ));
    }
    Ok(document)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NonBlankString(String);

text_deref!(NonBlankString);

impl NonBlankString {
    pub(crate) fn as_str(&self) -> &str {
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

/// Visible has one canonical spelling: absence. An explicit `false` says the same thing
/// and normalizes to it, so an aggregate comparison never reports a change that carries
/// no meaning.
pub(crate) fn normalize_optional_visibility(value: &mut Option<bool>) {
    *value = value.filter(|hidden| *hidden);
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

text_deref!(DateOrdered);

impl DateOrdered {
    pub(crate) fn as_str(&self) -> &str {
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

text_deref!(Currency);

impl Currency {
    pub(crate) fn as_str(&self) -> &str {
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
    pub(crate) fn new(text: impl Into<String>) -> Result<Self, MetadataError> {
        let text = text.into();
        if is_valid_index_order_directory(&text) {
            Ok(Self(text))
        } else {
            Err(MetadataError {
                kind: MetadataErrorKind::Shape,
                message: "expected a normalized relative slash-separated path".into(),
            })
        }
    }

    pub(crate) fn as_str(&self) -> &str {
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
        if is_valid_index_order_directory(&text) {
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

/// Read as the finite number it validated.
impl std::ops::Deref for FiniteNumber {
    type Target = f64;

    fn deref(&self) -> &f64 {
        &self.0
    }
}

impl FiniteNumber {
    pub(crate) fn new(value: f64) -> Option<Self> {
        value.is_finite().then_some(Self(value))
    }

    pub(crate) fn get(self) -> f64 {
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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
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
    /// Whether this Vendor is a Hidden Vendor: resolvable for every Order that references
    /// it, but not offered for new use. Absence means visible.
    #[serde(
        default,
        deserialize_with = "deserialize_present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub hidden: Option<bool>,
}

impl Vendor {
    /// Whether this is a Hidden Vendor. Absent visibility means visible, so this is the
    /// one place that reading has to be spelled out.
    pub fn is_hidden(&self) -> bool {
        self.hidden.unwrap_or(false)
    }
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
    #[serde(deserialize_with = "deserialize_indexed_order")]
    pub order: OrderDocument,
    pub readme: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OrderDocument {
    pub schema_version: Version1,
    #[serde(
        default = "PackageRootLocator::absent",
        skip_serializing_if = "PackageRootLocator::is_absent"
    )]
    pub package_root: PackageRootLocator,
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
    /// Multiline remarks tied to this Item alone.
    #[serde(
        default,
        deserialize_with = "deserialize_present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub notes: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderStatus {
    InPreparation,
    Hold,
    Ordered,
    Received,
    Complete,
}

impl crate::code::Coded for OrderStatus {}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MetadataErrorKind {
    Encoding,
    Syntax,
    Shape,
    Serialization,
}

#[derive(Debug, Serialize)]
pub struct MetadataError {
    kind: MetadataErrorKind,
    message: String,
}

impl MetadataError {
    pub(crate) fn kind(&self) -> MetadataErrorKind {
        self.kind
    }
}

impl std::fmt::Display for MetadataError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

macro_rules! impl_json_document {
    ($document:ty) => {
        impl $document {
            pub(crate) fn from_json_bytes(bytes: &[u8]) -> Result<Self, MetadataError> {
                from_json_bytes(bytes)
            }

            pub(crate) fn to_json_bytes(&self) -> Result<Vec<u8>, MetadataError> {
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
    pub(crate) fn from_toml_bytes(bytes: &[u8]) -> Result<Self, MetadataError> {
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

pub(crate) fn config_shape_field(bytes: &[u8]) -> Option<String> {
    let table = std::str::from_utf8(bytes)
        .ok()?
        .parse::<toml::Table>()
        .ok()?;
    for key in table.keys() {
        if key != "orders_path" && key != "removed_path" {
            return Some(key.clone());
        }
    }
    for key in ["orders_path", "removed_path"] {
        if table.get(key).is_some_and(|value| !value.is_str()) {
            return Some(key.into());
        }
    }
    None
}

pub(crate) fn version_shape_field(bytes: &[u8]) -> Option<String> {
    let value = serde_json::from_slice::<serde_json::Value>(bytes).ok()?;
    let object = value.as_object()?;
    if !object.contains_key("metadata_version") {
        return Some("metadata_version".into());
    }
    object
        .keys()
        .find(|key| key.as_str() != "metadata_version")
        .cloned()
        .or_else(|| (!object["metadata_version"].is_u64()).then(|| "metadata_version".into()))
}

/// The field that stopped an Order record from parsing.
///
/// **`package_root` is not one of them.** The locator carries its own outcome — see
/// [`PackageRootLocator`] — so a record never fails to parse for it, and naming it here
/// would attribute another field's failure to the locator whenever both are wrong at once.
pub(crate) fn order_shape_field(bytes: &[u8]) -> Option<String> {
    let value = serde_json::from_slice::<serde_json::Value>(bytes).ok()?;
    let object = value.as_object()?;
    let required = [
        "schema_version",
        "order_id",
        "title",
        "date_ordered",
        "status",
        "items",
    ];
    let known = [
        "schema_version",
        "package_root",
        "order_id",
        "title",
        "date_ordered",
        "status",
        "vendor_id",
        "orderer",
        "ordering_institution",
        "currency",
        "project",
        "order_group",
        "items",
    ];
    if let Some(field) = required.iter().find(|field| !object.contains_key(**field)) {
        return Some((*field).into());
    }
    if let Some(field) = object.keys().find(|field| !known.contains(&field.as_str())) {
        return Some(field.clone());
    }
    for (field, valid) in [
        ("schema_version", object["schema_version"].is_u64()),
        ("order_id", object["order_id"].is_string()),
        ("title", object["title"].is_string()),
        ("date_ordered", object["date_ordered"].is_string()),
        ("status", object["status"].is_string()),
    ] {
        if !valid {
            return Some(field.into());
        }
    }
    for field in [
        "vendor_id",
        "orderer",
        "ordering_institution",
        "currency",
        "project",
        "order_group",
    ] {
        if object.get(field).is_some_and(|value| !value.is_string()) {
            return Some(field.into());
        }
    }
    let Some(items) = object["items"].as_array() else {
        return Some("items".into());
    };
    for (index, item) in items.iter().enumerate() {
        let Some(item) = item.as_object() else {
            return Some(format!("items[{index}]"));
        };
        for field in ["description", "quantity", "unit_price"] {
            if !item.contains_key(field) {
                return Some(format!("items[{index}].{field}"));
            }
        }
        let known_item = [
            "description",
            "quantity",
            "unit_price",
            "catalog_number",
            "notes",
        ];
        if let Some(field) = item
            .keys()
            .find(|field| !known_item.contains(&field.as_str()))
        {
            return Some(format!("items[{index}].{field}"));
        }
        for (field, valid) in [
            (
                "description",
                serde_json::from_value::<NonBlankString>(item["description"].clone()).is_ok(),
            ),
            (
                "quantity",
                serde_json::from_value::<FiniteNumber>(item["quantity"].clone()).is_ok(),
            ),
            (
                "unit_price",
                serde_json::from_value::<FiniteNumber>(item["unit_price"].clone()).is_ok(),
            ),
        ] {
            if !valid {
                return Some(format!("items[{index}].{field}"));
            }
        }
        for field in ["catalog_number", "notes"] {
            if item.get(field).is_some_and(|value| !value.is_string()) {
                return Some(format!("items[{index}].{field}"));
            }
        }
    }
    for (field, valid) in [
        (
            "order_id",
            serde_json::from_value::<OrderId>(object["order_id"].clone()).is_ok(),
        ),
        (
            "title",
            serde_json::from_value::<NonBlankString>(object["title"].clone()).is_ok(),
        ),
        (
            "date_ordered",
            serde_json::from_value::<DateOrdered>(object["date_ordered"].clone()).is_ok(),
        ),
        (
            "status",
            serde_json::from_value::<OrderStatus>(object["status"].clone()).is_ok(),
        ),
    ] {
        if !valid {
            return Some(field.into());
        }
    }
    if object
        .get("currency")
        .is_some_and(|value| serde_json::from_value::<Currency>(value.clone()).is_err())
    {
        return Some("currency".into());
    }
    None
}

pub(crate) fn vendors_shape_field(bytes: &[u8]) -> Option<String> {
    let value = serde_json::from_slice::<serde_json::Value>(bytes).ok()?;
    let object = value.as_object()?;
    for field in ["schema_version", "vendors"] {
        if !object.contains_key(field) {
            return Some(field.into());
        }
    }
    if let Some(field) = object
        .keys()
        .find(|field| field.as_str() != "schema_version" && field.as_str() != "vendors")
    {
        return Some(field.clone());
    }
    if !object["schema_version"].is_u64() {
        return Some("schema_version".into());
    }
    let Some(vendors) = object["vendors"].as_array() else {
        return Some("vendors".into());
    };
    for (vendor_index, vendor) in vendors.iter().enumerate() {
        let Some(vendor) = vendor.as_object() else {
            return Some(format!("vendors[{vendor_index}]"));
        };
        for field in ["vendor_id", "name", "contacts"] {
            if !vendor.contains_key(field) {
                return Some(format!("vendors[{vendor_index}].{field}"));
            }
        }
        let known_vendor = [
            "vendor_id",
            "name",
            "phone",
            "email",
            "website",
            "address",
            "contacts",
            "purchasing_notes",
            "hidden",
        ];
        if let Some(field) = vendor
            .keys()
            .find(|field| !known_vendor.contains(&field.as_str()))
        {
            return Some(format!("vendors[{vendor_index}].{field}"));
        }
        for (field, valid) in [
            (
                "vendor_id",
                serde_json::from_value::<VendorId>(vendor["vendor_id"].clone()).is_ok(),
            ),
            (
                "name",
                serde_json::from_value::<NonBlankString>(vendor["name"].clone()).is_ok(),
            ),
        ] {
            if !valid {
                return Some(format!("vendors[{vendor_index}].{field}"));
            }
        }
        for field in ["phone", "email", "website", "address", "purchasing_notes"] {
            if vendor.get(field).is_some_and(|value| !value.is_string()) {
                return Some(format!("vendors[{vendor_index}].{field}"));
            }
        }
        if vendor
            .get("hidden")
            .is_some_and(|value| !value.is_boolean())
        {
            return Some(format!("vendors[{vendor_index}].hidden"));
        }
        let Some(contacts) = vendor["contacts"].as_array() else {
            return Some(format!("vendors[{vendor_index}].contacts"));
        };
        for (contact_index, contact) in contacts.iter().enumerate() {
            let prefix = format!("vendors[{vendor_index}].contacts[{contact_index}]");
            let Some(contact) = contact.as_object() else {
                return Some(prefix);
            };
            if !contact.contains_key("name") {
                return Some(format!("{prefix}.name"));
            }
            let known_contact = ["name", "role", "phone", "email"];
            if let Some(field) = contact
                .keys()
                .find(|field| !known_contact.contains(&field.as_str()))
            {
                return Some(format!("{prefix}.{field}"));
            }
            if serde_json::from_value::<NonBlankString>(contact["name"].clone()).is_err() {
                return Some(format!("{prefix}.name"));
            }
            for field in ["role", "phone", "email"] {
                if contact.get(field).is_some_and(|value| !value.is_string()) {
                    return Some(format!("{prefix}.{field}"));
                }
            }
        }
    }
    None
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

fn is_valid_index_order_directory(text: &str) -> bool {
    !text.is_empty()
        && !text.starts_with('/')
        && !matches!(text.as_bytes(), [drive, b':', ..] if drive.is_ascii_alphabetic())
        && !text.contains('\\')
        && text
            .split('/')
            .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
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

#[cfg(test)]
mod tests;
