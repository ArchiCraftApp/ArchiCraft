//! The IFCX container (`schema/ifcx.tsp` in SRS R2): header, imports,
//! schemas and data nodes.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A whole IFCX document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct IfcxFile {
    pub header: Header,
    pub imports: Vec<Import>,
    /// Attribute schemas declared in the file, by attribute ID. Kept as JSON
    /// so that unknown schema features do not prevent reading the file; see
    /// [`SchemaEntry::parse`].
    pub schemas: IndexMap<String, Value>,
    pub data: Vec<Node>,
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
pub struct Node {
    pub path: String,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub children: IndexMap<String, Option<String>>,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub inherits: IndexMap<String, Option<String>>,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub attributes: IndexMap<String, Value>,
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
