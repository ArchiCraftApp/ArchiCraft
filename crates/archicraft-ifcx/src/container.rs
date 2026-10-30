//! The IFCX container (`schema/ifcx.tsp` in SRS R2): header, imports,
//! schemas and data nodes.
//!
//! The data nodes hold their attribute values as [`Value`] trees by default.
//! Opening a file holds them as raw JSON text borrowed from the file's bytes
//! instead (`IfcxFile<&RawValue>`, see
//! [`validate::parse_raw`](crate::validate::parse_raw)): a tree of a mesh
//! takes more than ten times the bytes of its text, so a file held as trees
//! needs far more memory than its size (NFR-SEC-003). An attribute is
//! checked against its schema while its text is read, without a tree, and
//! parsed when it is read, one at a time ([`AttributeValue`]).

use std::borrow::Cow;
use std::fmt;

use indexmap::IndexMap;
use serde::de::{self, DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_json::value::RawValue;

#[cfg(doc)]
use crate::validate;

/// A whole IFCX document, with attribute values of type `A`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct IfcxFile<A = Value> {
    pub header: Header,
    pub imports: Vec<Import>,
    /// Attribute schemas declared in the file, by attribute ID. Kept as JSON
    /// so that unknown schema features do not prevent reading the file; see
    /// [`SchemaEntry::parse`].
    pub schemas: IndexMap<String, Value>,
    pub data: Vec<Node<A>>,
}

impl<A: AttributeValue> IfcxFile<A> {
    /// The file with every attribute value as a [`Value`] tree.
    pub fn to_values(&self) -> Result<IfcxFile, serde_json::Error> {
        let data = self
            .data
            .iter()
            .map(|node| {
                let attributes = node
                    .attributes
                    .iter()
                    .map(|(id, value)| Ok((id.clone(), value.value()?.into_owned())))
                    .collect::<Result<_, serde_json::Error>>()?;
                Ok(Node {
                    path: node.path.clone(),
                    children: node.children.clone(),
                    inherits: node.inherits.clone(),
                    attributes,
                })
            })
            .collect::<Result<_, serde_json::Error>>()?;
        Ok(IfcxFile {
            header: self.header.clone(),
            imports: self.imports.clone(),
            schemas: self.schemas.clone(),
            data,
        })
    }
}

/// The type of an attribute value in [`IfcxFile`] and
/// [`FlatNode`](crate::validate::FlatNode): a [`Value`] tree, or the value's
/// raw JSON text, which costs a pointer and a length until it is read
/// (NFR-SEC-003).
pub trait AttributeValue: Clone {
    /// The value as a tree: borrowed when it is one, parsed when it is raw
    /// text. The parsed tree is the caller's to drop once it is read.
    fn value(&self) -> Result<Cow<'_, Value>, serde_json::Error>;

    /// The field `key` of the value as a tree, if the value is an object:
    /// `None` when it is not one, `Some(None)` when it has no such field. Of
    /// raw text, only the object's top level and the field are parsed, so
    /// that reading one field of a large object does not parse the others.
    fn field(&self, key: &str) -> Result<Option<Option<Cow<'_, Value>>>, serde_json::Error>;

    /// The value's raw JSON text, if it is raw text: validation checks it
    /// while it reads it, without a tree of it (NFR-SEC-003,
    /// [`validate::check_attributes`]).
    fn raw(&self) -> Option<&RawValue>;
}

/// The keys by which `serde_json` reads a [`Value`] differently from other
/// keys, when such a key comes first in an object (`serde_json::raw::TOKEN`,
/// and `serde_json::number::TOKEN` with the feature `arbitrary_precision`),
/// start with this prefix.
pub(crate) const SERDE_JSON_TOKEN: &str = "$serde_json::private::";

/// The key by which `serde_json` reads an object as a [`Value`] differently
/// (`serde_json::raw::TOKEN`, with the feature `raw_value`): an object whose
/// first key it is stands for the JSON value in the text of that key's
/// string ([`read_raw_value_token`]).
pub(crate) const RAW_VALUE_TOKEN: &str = "$serde_json::private::RawValue";

/// An object key, borrowed from the text when it holds no escapes. Every
/// visitor of raw text compares an object's first key with
/// [`RAW_VALUE_TOKEN`].
pub(crate) struct Key;

impl<'de> DeserializeSeed<'de> for Key {
    type Value = Cow<'de, str>;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_str(KeyVisitor("a string key"))
    }
}

/// Reads a string as `serde_json` reads keys (its expectation is the one
/// that `serde_json` names in its message when the value is no string).
struct KeyVisitor(&'static str);

impl<'de> Visitor<'de> for KeyVisitor {
    type Value = Cow<'de, str>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str(self.0)
    }

    fn visit_borrowed_str<E>(self, text: &'de str) -> Result<Self::Value, E> {
        Ok(Cow::Borrowed(text))
    }

    fn visit_str<E>(self, text: &str) -> Result<Self::Value, E> {
        Ok(Cow::Owned(text.to_owned()))
    }
}

/// Reads the rest of an object whose first key was [`RAW_VALUE_TOKEN`] as a
/// [`Value`] reads it, with the same messages: the key's value is a string,
/// the object is the value in the JSON text of that string, read with
/// `seed`, and the object ends there.
pub(crate) fn read_raw_value_token<'de, M: MapAccess<'de>, T>(
    map: &mut M,
    seed: impl for<'a> DeserializeSeed<'a, Value = T>,
) -> Result<T, M::Error> {
    struct Text;

    impl<'de> DeserializeSeed<'de> for Text {
        type Value = Cow<'de, str>;

        fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
            deserializer.deserialize_str(KeyVisitor("raw value"))
        }
    }

    let text = map.next_value_seed(Text)?;
    let mut inner = serde_json::Deserializer::from_str(&text);
    seed.deserialize(&mut inner)
        .and_then(|value| inner.end().map(|()| value))
        .map_err(de::Error::custom)
}

/// Any JSON value read as a [`Value`] reads it, with the same nesting limit,
/// number ranges, string checks and [`RAW_VALUE_TOKEN`], without keeping
/// anything.
pub(crate) struct Skip;

impl<'de> DeserializeSeed<'de> for Skip {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Skip {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("any valid JSON value")
    }

    fn visit_bool<E>(self, _: bool) -> Result<(), E> {
        Ok(())
    }

    fn visit_i64<E>(self, _: i64) -> Result<(), E> {
        Ok(())
    }

    fn visit_u64<E>(self, _: u64) -> Result<(), E> {
        Ok(())
    }

    fn visit_f64<E>(self, _: f64) -> Result<(), E> {
        Ok(())
    }

    fn visit_str<E>(self, _: &str) -> Result<(), E> {
        Ok(())
    }

    fn visit_unit<E>(self) -> Result<(), E> {
        Ok(())
    }

    fn visit_seq<S: SeqAccess<'de>>(self, mut seq: S) -> Result<(), S::Error> {
        while seq.next_element_seed(Skip)?.is_some() {}
        Ok(())
    }

    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<(), M::Error> {
        let first = map.next_key_seed(Key)?;
        if first.as_deref() == Some(RAW_VALUE_TOKEN) {
            return read_raw_value_token(&mut map, Skip);
        }
        let mut key = first;
        while key.is_some() {
            map.next_value_seed(Skip)?;
            key = map.next_key_seed(Key)?;
        }
        Ok(())
    }
}

impl AttributeValue for Value {
    fn value(&self) -> Result<Cow<'_, Value>, serde_json::Error> {
        Ok(Cow::Borrowed(self))
    }

    fn field(&self, key: &str) -> Result<Option<Option<Cow<'_, Value>>>, serde_json::Error> {
        Ok(self.as_object().map(|object| object.get(key).map(Cow::Borrowed)))
    }

    fn raw(&self) -> Option<&RawValue> {
        None
    }
}

impl AttributeValue for &RawValue {
    fn value(&self) -> Result<Cow<'_, Value>, serde_json::Error> {
        serde_json::from_str(self.get()).map(Cow::Owned)
    }

    fn field(&self, key: &str) -> Result<Option<Option<Cow<'_, Value>>>, serde_json::Error> {
        if !self.get().starts_with('{') {
            return Ok(None);
        }
        // Later fields of the same name win, as in a tree.
        let fields: IndexMap<String, &RawValue> = serde_json::from_str(self.get())?;
        if fields.keys().next().is_some_and(|k| k.starts_with(SERDE_JSON_TOKEN)) {
            let tree: Value = serde_json::from_str(self.get())?;
            return Ok(tree.as_object().map(|object| object.get(key).cloned().map(Cow::Owned)));
        }
        let field = fields.get(key).map(|raw| serde_json::from_str(raw.get()));
        Ok(Some(field.transpose()?.map(Cow::Owned)))
    }

    fn raw(&self) -> Option<&RawValue> {
        Some(self)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Header {
    pub id: String,
    pub ifcx_version: String,
    pub data_version: String,
    pub author: String,
    pub timestamp: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Import {
    pub uri: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub integrity: Option<String>,
}

/// One data entry. Several entries may address the same path; later entries
/// win attribute by attribute when the file is composed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Node<A = Value> {
    pub path: String,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub children: IndexMap<String, Option<String>>,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub inherits: IndexMap<String, Option<String>>,
    // A plain `default` would make serde require `A: Default`, which a
    // borrowed `&RawValue` is not.
    #[serde(default = "IndexMap::new", skip_serializing_if = "IndexMap::is_empty")]
    pub attributes: IndexMap<String, A>,
}

impl Node {
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            children: IndexMap::new(),
            inherits: IndexMap::new(),
            attributes: IndexMap::new(),
        }
    }
}

/// Data types of attribute values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DataType {
    Real,
    Boolean,
    Integer,
    String,
    DateTime,
    Enum,
    Array,
    Object,
    Reference,
    Blob,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EnumRestrictions {
    pub options: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArrayRestrictions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    pub value: ValueDescription,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ObjectRestrictions {
    pub values: IndexMap<String, ValueDescription>,
}

/// Describes the allowed values of an attribute or attribute field.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValueDescription {
    pub data_type: DataType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub optional: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inherits: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quantity_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enum_restrictions: Option<EnumRestrictions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub array_restrictions: Option<Box<ArrayRestrictions>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object_restrictions: Option<ObjectRestrictions>,
}

/// A schema declaration: the description of one attribute.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SchemaEntry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    pub value: ValueDescription,
}

impl SchemaEntry {
    pub fn parse(value: &Value) -> Result<Self, serde_json::Error> {
        Self::deserialize(value)
    }
}
