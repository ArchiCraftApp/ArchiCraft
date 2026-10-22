//! The IFCX container (`schema/ifcx.tsp` in SRS R2): header, imports,
//! schemas and data nodes.
//!
//! The data nodes hold their attribute values as [`Value`] trees by default.
//! Opening a file holds them as raw JSON text borrowed from the file's bytes
//! instead (`IfcxFile<&RawValue>`, see
//! [`validate::parse_raw`](crate::validate::parse_raw)): a tree of a mesh
//! takes more than ten times the bytes of its text, so a file held as trees
//! needs far more memory than its size (NFR-SEC-003). An attribute is parsed
//! when it is validated or read, one at a time ([`AttributeValue`]).

use std::borrow::Cow;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_json::value::RawValue;

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
}

/// The keys by which `serde_json` reads a [`Value`] differently from other
/// keys, when such a key comes first in an object (`serde_json::raw::TOKEN`,
/// and `serde_json::number::TOKEN` with the feature `arbitrary_precision`),
/// start with this prefix.
pub(crate) const SERDE_JSON_TOKEN: &str = "$serde_json::private::";

impl AttributeValue for Value {
    fn value(&self) -> Result<Cow<'_, Value>, serde_json::Error> {
        Ok(Cow::Borrowed(self))
    }

    fn field(&self, key: &str) -> Result<Option<Option<Cow<'_, Value>>>, serde_json::Error> {
        Ok(self.as_object().map(|object| object.get(key).map(Cow::Borrowed)))
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
