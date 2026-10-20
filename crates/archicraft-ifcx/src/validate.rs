//! Validation of IFCX documents, following the reference implementation in
//! `src/ifcx-core` of SRS R2 (IFX-005, IFX-006, IFX-083), plus the parser
//! limits of SRS Appendix D.9.
//!
//! Composition: entries with the same path are flattened in file order, later
//! entries winning attribute by attribute; a `null` inheritance is removed.
//! Every attribute of every flattened node must be declared by an imported or
//! in-file schema and must match it. The composed tree must have no cycles.

use std::collections::{HashMap, HashSet};
use std::io::Read;

use indexmap::IndexMap;
use serde_json::Value;

use crate::container::{DataType, IfcxFile, SchemaEntry, ValueDescription};
use crate::registry;

/// Largest accepted file: 2 GB (Appendix D.9).
pub const MAX_FILE_SIZE: usize = 2 * 1024 * 1024 * 1024;
/// Largest number of imports (Appendix D.9).
pub const MAX_IMPORTS: usize = 32;
/// Deepest `children`/`inherits` chain (Appendix D.9).
pub const MAX_COMPOSITION_DEPTH: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

/// A problem found while validating or reading a file, with the affected node
/// path where there is one (IFX-083, PRJ-008).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    pub severity: Severity,
    pub path: Option<String>,
    pub message: String,
}

impl Problem {
    pub fn error(path: Option<&str>, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            path: path.map(str::to_owned),
            message: message.into(),
        }
    }

    pub fn warning(path: Option<&str>, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warning,
            path: path.map(str::to_owned),
            message: message.into(),
        }
    }

    pub fn info(path: Option<&str>, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Info,
            path: path.map(str::to_owned),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.path {
            Some(path) => write!(f, "[{path}] {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

/// A node after flattening all entries with its path.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FlatNode {
    pub children: IndexMap<String, Option<String>>,
    pub inherits: IndexMap<String, String>,
    pub attributes: IndexMap<String, Value>,
}

/// Flattens the data entries per path, in file order.
pub fn flatten(file: &IfcxFile) -> IndexMap<String, FlatNode> {
    let mut nodes: IndexMap<String, FlatNode> = IndexMap::new();
    for entry in &file.data {
        let node = nodes.entry(entry.path.clone()).or_default();
        for (name, child) in &entry.children {
            node.children.insert(name.clone(), child.clone());
        }
        for (name, inherit) in &entry.inherits {
            match inherit {
                Some(target) => {
                    node.inherits.insert(name.clone(), target.clone());
                }
                None => {
                    node.inherits.shift_remove(name);
                }
            }
        }
        for (name, value) in &entry.attributes {
            node.attributes.insert(name.clone(), value.clone());
        }
    }
    nodes
}

/// The attribute schemas visible to a file: its own declarations, then the
/// imports it names.
#[derive(Debug)]
pub struct SchemaSet {
    schemas: HashMap<String, ValueDescription>,
}

impl SchemaSet {
    /// Collects schemas, reporting imports that cannot be resolved locally
    /// and declarations that cannot be parsed. An import is resolved from
    /// `extra` (import URI, schema document) before the bundled registry, so
    /// that a test can resolve a schema ArchiCraft does not ship (IFX-092).
    pub fn for_file(file: &IfcxFile, extra: &[(&str, &IfcxFile)], problems: &mut Vec<Problem>) -> Self {
        let mut schemas = HashMap::new();
        for (id, value) in &file.schemas {
            match SchemaEntry::parse(value) {
                Ok(entry) => {
                    schemas.insert(id.clone(), entry.value);
                }
                Err(e) => problems.push(Problem::error(None, format!("Schema “{id}” cannot be read: {e}"))),
            }
        }
        for import in &file.imports {
            let source = extra
                .iter()
                .find(|(uri, _)| *uri == import.uri)
                .map(|(_, file)| *file)
                .or_else(|| registry::bundled(&import.uri));
            match source {
                Some(source) => {
                    for (id, value) in &source.schemas {
                        if let Ok(entry) = SchemaEntry::parse(value) {
                            schemas.insert(id.clone(), entry.value);
                        }
                    }
                }
                None => problems.push(Problem::warning(
                    None,
                    format!(
                        "Import “{}” is not available offline; its attributes cannot be checked.",
                        import.uri
                    ),
                )),
            }
        }
        Self { schemas }
    }

    pub fn get(&self, id: &str) -> Option<&ValueDescription> {
        self.schemas.get(id)
    }
}

fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Checks a value against its description, as the reference implementation
/// does. Returns a message on the first mismatch.
pub fn check_value(desc: &ValueDescription, value: &Value, schemas: &SchemaSet, depth: usize) -> Result<(), String> {
    if depth > 64 {
        return Err("schema nesting is too deep".into());
    }
    if let Some(inherited) = &desc.inherits {
        for id in inherited {
            let parent = schemas
                .get(id)
                .ok_or_else(|| format!("unknown inherited schema “{id}”"))?;
            check_value(parent, value, schemas, depth + 1)?;
        }
    }
    let expect = |ok: bool, what: &str| {
        if ok {
            Ok(())
        } else {
            Err(format!("expected {what}, found {}", type_name(value)))
        }
    };
    match desc.data_type {
        DataType::Boolean => expect(value.is_boolean(), "a boolean"),
        DataType::String | DataType::DateTime | DataType::Reference => expect(value.is_string(), "a string"),
        DataType::Integer | DataType::Real => expect(value.is_number(), "a number"),
        DataType::Enum => {
            let text = value
                .as_str()
                .ok_or_else(|| format!("expected a string, found {}", type_name(value)))?;
            let options = desc
                .enum_restrictions
                .as_ref()
                .map(|r| r.options.as_slice())
                .unwrap_or_default();
            if options.iter().filter(|o| *o == text).count() == 1 {
                Ok(())
            } else {
                Err(format!("“{text}” is not one of [{}]", options.join(",")))
            }
        }
        DataType::Object => {
            let object = value
                .as_object()
                .ok_or_else(|| format!("expected an object, found {}", type_name(value)))?;
            if let Some(restrictions) = &desc.object_restrictions {
                for (key, field) in &restrictions.values {
                    match object.get(key) {
                        None if field.optional == Some(true) => {}
                        None => return Err(format!("missing key “{key}”")),
                        Some(v) => check_value(field, v, schemas, depth + 1).map_err(|e| format!("{key}: {e}"))?,
                    }
                }
            }
            Ok(())
        }
        DataType::Array => {
            let items = value
                .as_array()
                .ok_or_else(|| format!("expected an array, found {}", type_name(value)))?;
            let restrictions = desc
                .array_restrictions
                .as_ref()
                .ok_or("an Array schema needs arrayRestrictions")?;
            for item in items {
                check_value(&restrictions.value, item, schemas, depth + 1)?;
            }
            Ok(())
        }
        DataType::Blob => Err("the Blob data type is not supported".into()),
    }
}

/// Checks that every attribute is declared and valid (IFX-005).
pub fn check_attributes(nodes: &IndexMap<String, FlatNode>, schemas: &SchemaSet) -> Vec<Problem> {
    let mut problems = Vec::new();
    for (path, node) in nodes {
        for (id, value) in &node.attributes {
            match schemas.get(id) {
                None => problems.push(Problem::error(Some(path), format!("attribute “{id}” is not declared"))),
                Some(desc) => {
                    if let Err(e) = check_value(desc, value, schemas, 0) {
                        problems.push(Problem::error(Some(path), format!("attribute “{id}”: {e}")));
                    }
                }
            }
        }
    }
    problems
}

/// The roots of the composed tree (paths that nothing refers to), or an error
/// if `children` or `inherits` form a cycle or are nested too deeply.
pub fn roots(nodes: &IndexMap<String, FlatNode>) -> Result<Vec<String>, Problem> {
    let mut referenced: HashSet<&str> = HashSet::new();
    for node in nodes.values() {
        referenced.extend(node.children.values().flatten().map(String::as_str));
        referenced.extend(node.inherits.values().map(String::as_str));
    }
    // Depth-first search with explicit states: 1 = on the stack, 2 = done.
    let mut state: HashMap<&str, (u8, usize)> = HashMap::new();
    fn visit<'a>(
        path: &'a str,
        nodes: &'a IndexMap<String, FlatNode>,
        state: &mut HashMap<&'a str, (u8, usize)>,
        level: usize,
    ) -> Result<usize, Problem> {
        if level > MAX_COMPOSITION_DEPTH {
            return Err(Problem::error(Some(path), "composition is nested too deeply"));
        }
        match state.get(path) {
            Some((1, _)) => return Err(Problem::error(Some(path), "composition cycle")),
            Some((_, depth)) => return Ok(*depth),
            None => {}
        }
        state.insert(path, (1, 0));
        let mut depth = 0;
        if let Some(node) = nodes.get(path) {
            let head = |p: &'a str| p.split('/').next().unwrap_or(p);
            for target in node.children.values().flatten().chain(node.inherits.values()) {
                depth = depth.max(1 + visit(head(target), nodes, state, level + 1)?);
            }
        }
        if depth > MAX_COMPOSITION_DEPTH {
            return Err(Problem::error(Some(path), "composition is nested too deeply"));
        }
        state.insert(path, (2, depth));
        Ok(depth)
    }
    let mut roots = Vec::new();
    for path in nodes.keys() {
        visit(path, nodes, &mut state, 0)?;
        if !referenced.contains(path.as_str()) && !path.contains('/') {
            roots.push(path.clone());
        }
    }
    Ok(roots)
}

/// Reports `children` and `inherits` targets that are not in the file.
pub fn dangling_references(nodes: &IndexMap<String, FlatNode>) -> Vec<Problem> {
    let mut problems = Vec::new();
    for (path, node) in nodes {
        for target in node.children.values().flatten().chain(node.inherits.values()) {
            let head = target.split('/').next().unwrap_or(target);
            if !nodes.contains_key(head) {
                problems.push(Problem::warning(
                    Some(path),
                    format!("refers to the unknown node “{target}”"),
                ));
            }
        }
    }
    problems
}

/// The message for a file above the size limit, naming the limit in
/// user-facing units (NFR-SEC-003).
fn too_large(limit: u64) -> Problem {
    const KB: u64 = 1024;
    let size = match limit {
        l if l >= KB * KB * KB && l % (KB * KB * KB) == 0 => format!("{} GB", l / (KB * KB * KB)),
        l if l >= KB * KB && l % (KB * KB) == 0 => format!("{} MB", l / (KB * KB)),
        l if l >= KB && l % KB == 0 => format!("{} KB", l / KB),
        l => format!("{l} bytes"),
    };
    Problem::error(None, format!("the file is larger than the limit of {size}"))
}

/// Reads a whole file of at most `limit` bytes (Appendix D.9 file size,
/// NFR-SEC-003). A `declared_len` above the limit is rejected before
/// anything is read; the read itself stops one byte past the limit, so a
/// file that grows while it is read, or whose length is unknown (`None`, for
/// example a pipe), is rejected too.
pub fn read_bounded(reader: impl Read, declared_len: Option<u64>, limit: u64) -> Result<Vec<u8>, Problem> {
    if declared_len.is_some_and(|len| len > limit) {
        return Err(too_large(limit));
    }
    let capacity = declared_len.unwrap_or(0).min(limit);
    let mut bytes = Vec::with_capacity(usize::try_from(capacity).unwrap_or(0));
    reader
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|e| Problem::error(None, format!("the file cannot be read: {e}")))?;
    if bytes.len() as u64 > limit {
        return Err(too_large(limit));
    }
    Ok(bytes)
}

/// Parses an IFCX document from bytes, enforcing the limits of Appendix D.9
/// (file size, JSON nesting below 128 levels, imports; NFR-SEC-003). The
/// JSON nesting limit is the recursion limit of `serde_json`.
pub fn parse(bytes: &[u8]) -> Result<IfcxFile, Problem> {
    if bytes.len() > MAX_FILE_SIZE {
        return Err(too_large(MAX_FILE_SIZE as u64));
    }
    let file: IfcxFile = serde_json::from_slice(bytes).map_err(|e| {
        // serde_json reports its recursion limit only in the message.
        if e.to_string().starts_with("recursion limit exceeded") {
            Problem::error(None, "the file is nested more than 127 levels deep")
        } else {
            Problem::error(None, format!("the file is not a valid IFCX document: {e}"))
        }
    })?;
    if file.imports.len() > MAX_IMPORTS {
        return Err(Problem::error(None, "the file has more than 32 imports"));
    }
    Ok(file)
}

/// Full validation of a parsed document, as the reference implementation
/// loads it with schema checks enabled (IFX-006 a), plus a single-root check
/// (IFX-013) and dangling references (IFX-083).
pub fn validate(file: &IfcxFile) -> Vec<Problem> {
    validate_with(file, &[])
}

/// [`validate`] with extra schema documents by import URI, resolved before
/// the bundled ones. Production code validates against the bundled registry
/// only; this lets a test check a file that imports a schema not bundled,
/// such as a published `archicraft::` schema (IFX-092).
pub fn validate_with(file: &IfcxFile, extra: &[(&str, &IfcxFile)]) -> Vec<Problem> {
    let mut problems = Vec::new();
    let schemas = SchemaSet::for_file(file, extra, &mut problems);
    let nodes = flatten(file);
    problems.extend(check_attributes(&nodes, &schemas));
    match roots(&nodes) {
        Ok(roots) if roots.len() == 1 => {}
        Ok(roots) => problems.push(Problem::warning(
            None,
            format!("the composed tree has {} roots instead of one", roots.len()),
        )),
        Err(e) => problems.push(e),
    }
    problems.extend(dangling_references(&nodes));
    problems
}
