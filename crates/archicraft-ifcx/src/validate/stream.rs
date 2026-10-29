//! Checking a raw attribute value against its description while its text
//! is read (IFX-005, NFR-SEC-003): the checks of [`check_value`], in its
//! order and with its messages, without a [`Value`](serde_json::Value)
//! tree of the attribute, which takes tens of times the text of a mesh.
//!
//! `check_value` checks a value against the descriptions that a description
//! inherits, then against the description's own type, then an array's items
//! and an object's fields against theirs, each in turn, and returns the
//! first mismatch. Read in one pass, a value meets all of these checks at
//! once. A [`Plan`] holds the checks of one place in the value (the
//! attribute, the items of an array in it, a field of an object in it), each
//! once, and the order in which `check_value` makes them for each check of
//! the outer place ([`Step`]); the result of every check is kept until the
//! value is read, so that the first mismatch is the one `check_value`
//! returns. The plans of the inner places are made when a value reaches
//! them, once per place and file, not per value: [`Checks`] keeps the plan of
//! each attribute's description for all attributes of a file, and each place
//! lends the buffer of its results to every value read there, so that
//! checking allocates nothing per value.
//!
//! The value is read to its end as a tree is, with the same nesting limit,
//! number ranges, string checks and keys ([`RAW_VALUE_TOKEN`]), so that text
//! that does not parse gets the message of the tree parser.

use std::cell::{Cell, OnceCell};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::ptr;

use indexmap::IndexMap;
use serde::de::{DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor};

#[cfg(doc)]
use super::check_value;
use super::{MAX_SCHEMA_DEPTH, SchemaSet};
use crate::container::{DataType, Key, RAW_VALUE_TOKEN, Skip, ValueDescription, read_raw_value_token};
use crate::message::Message;

/// Checks raw attribute values against the descriptions of one schema set,
/// keeping the plan of each description for the next value it checks.
pub(super) struct Checks<'s> {
    schemas: &'s SchemaSet,
    /// The place of a whole attribute value, by its description.
    attributes: HashMap<*const ValueDescription, Inner<'s>>,
}

impl<'s> Checks<'s> {
    pub(super) fn new(schemas: &'s SchemaSet) -> Self {
        Self {
            schemas,
            attributes: HashMap::new(),
        }
    }

    /// Checks the raw JSON text of a value against `desc` as `check_value`
    /// checks its tree. Text that does not parse as a tree gets the parser's
    /// message, as `ifcx-detail`.
    pub(super) fn check(&mut self, desc: &'s ValueDescription, text: &str) -> Result<(), Message> {
        let schemas = self.schemas;
        let attribute = self
            .attributes
            .entry(ptr::from_ref(desc))
            .or_insert_with(|| Inner::new([(0, desc, 0)], schemas));
        let mut results = attribute.lend_results();
        let mut deserializer = serde_json::Deserializer::from_str(text);
        let read = Check {
            plan: &attribute.plan,
            schemas,
            out: &mut results,
        }
        .deserialize(&mut deserializer)
        .and_then(|()| deserializer.end());
        let mismatch = match attribute.groups.first() {
            Some((_, steps)) => first_mismatch(steps, &results),
            None => None,
        };
        attribute.results.set(results);
        read.map_err(|e| Message::new("ifcx-detail").text("detail", e.to_string()))?;
        mismatch.map_or(Ok(()), Err)
    }
}

/// A check of a value against a description's own type at a nesting depth
/// of `check_value`, without the descriptions it inherits, which are checks
/// of their own.
#[derive(Clone, Copy)]
struct Own<'s> {
    desc: &'s ValueDescription,
    depth: usize,
}

/// One step of `check_value` on a description: one of the checks of the
/// place, by its index, or a mismatch whatever the value is, after which
/// `check_value` stops.
#[derive(Clone)]
enum Step {
    Check(usize),
    Fail(Message),
}

/// The checks of one place in a value, each once, and lazily the places in
/// it: the items of the arrays there, and the fields of the objects.
#[derive(Default)]
struct Plan<'s> {
    checks: Vec<Own<'s>>,
    items: OnceCell<Box<Inner<'s>>>,
    fields: OnceCell<Fields<'s>>,
}

/// A place inside the values of an outer place: its plan, and the steps of
/// each outer check that checks it (an array check its items, an object
/// check one of its fields), with the outer check's index, in the order
/// [`Inner::new`] was given them.
struct Inner<'s> {
    plan: Plan<'s>,
    groups: Vec<(usize, Vec<Step>)>,
    /// The buffer of the results of the place's checks, lent to each value
    /// read there ([`Inner::lend_results`]).
    results: Cell<Vec<Option<Message>>>,
}

/// The fields of the objects at a place that its checks restrict.
struct Fields<'s> {
    /// The place of each field, with the first of the slots of its groups
    /// among an object's results.
    places: IndexMap<&'s str, (Inner<'s>, usize)>,
    /// The slots of all groups.
    slots: usize,
    /// For each object check with restrictions, by its index: the place
    /// and the group of each of its fields, in the order of its
    /// restrictions.
    order: Vec<(usize, Vec<(usize, usize)>)>,
    /// The buffer of the slots, lent to each object read at the place.
    read: Cell<Vec<Option<Option<Message>>>>,
}

impl<'s> Inner<'s> {
    /// The place that each of `outer` (an outer check's index, a
    /// description and its depth) checks with `check_value`.
    fn new(outer: impl IntoIterator<Item = (usize, &'s ValueDescription, usize)>, schemas: &'s SchemaSet) -> Self {
        let mut checks = Vec::new();
        let mut index = HashMap::new();
        let groups = outer
            .into_iter()
            .map(|(outer, desc, depth)| {
                let mut expansion = Expansion {
                    schemas,
                    checks: &mut checks,
                    index: &mut index,
                    steps: Vec::new(),
                    expanded: HashSet::new(),
                };
                expansion.expand(desc, depth);
                (outer, expansion.steps)
            })
            .collect();
        Self {
            plan: Plan {
                checks,
                ..Plan::default()
            },
            groups,
            results: Cell::default(),
        }
    }

    /// The buffer for the results of the place's checks, one per check, to
    /// be given back with `results.set` once they are read. A value
    /// overwrites all of them. A buffer not given back, after an error, is
    /// made again.
    fn lend_results(&self) -> Vec<Option<Message>> {
        let mut results = self.results.take();
        results.resize(self.plan.checks.len(), None);
        results
    }
}

/// The steps of `check_value` on one description, as they are added to a
/// place.
struct Expansion<'a, 's> {
    schemas: &'s SchemaSet,
    checks: &'a mut Vec<Own<'s>>,
    /// The index of each check of the place, by description and depth.
    index: &'a mut HashMap<(*const ValueDescription, usize), usize>,
    steps: Vec<Step>,
    /// The descriptions and depths whose steps are in `steps` already.
    expanded: HashSet<(*const ValueDescription, usize)>,
}

impl<'s> Expansion<'_, 's> {
    /// Adds the steps of `check_value(desc, _, depth)`, the descriptions it
    /// inherits first. Returns false after a step that fails whatever the
    /// value is, where `check_value` stops.
    fn expand(&mut self, desc: &'s ValueDescription, depth: usize) -> bool {
        // A description inherited again at the same depth adds no step: its
        // steps are there already, and any of them fails there first. So a
        // description inheriting another twice on every level costs its
        // number of levels, not two to their power.
        if !self.expanded.insert((ptr::from_ref(desc), depth)) {
            return true;
        }
        if depth > MAX_SCHEMA_DEPTH {
            self.steps.push(Step::Fail(Message::new("ifcx-schema-too-deep")));
            return false;
        }
        for id in desc.inherits.iter().flatten() {
            let Some(parent) = self.schemas.get(id) else {
                self.steps.push(Step::Fail(
                    Message::new("ifcx-schema-unknown-inherited").text("id", id.clone()),
                ));
                return false;
            };
            if !self.expand(parent, depth + 1) {
                return false;
            }
        }
        let checks = &mut *self.checks;
        let index = *self.index.entry((ptr::from_ref(desc), depth)).or_insert_with(|| {
            checks.push(Own { desc, depth });
            checks.len() - 1
        });
        self.steps.push(Step::Check(index));
        true
    }
}

impl<'s> Plan<'s> {
    /// The place of the items of the arrays here, for the array checks with
    /// restrictions.
    fn items(&self, schemas: &'s SchemaSet) -> &Inner<'s> {
        self.items.get_or_init(|| {
            let outer = self.checks.iter().enumerate().filter_map(|(i, own)| {
                let desc = own.desc;
                match (desc.data_type, desc.array_restrictions.as_deref()) {
                    (DataType::Array, Some(restrictions)) => Some((i, &restrictions.value, own.depth + 1)),
                    _ => None,
                }
            });
            Box::new(Inner::new(outer, schemas))
        })
    }

    /// The places of the fields that the object checks here restrict.
    fn fields(&self, schemas: &'s SchemaSet) -> &Fields<'s> {
        self.fields.get_or_init(|| {
            let mut outer: IndexMap<&'s str, Vec<(usize, &'s ValueDescription, usize)>> = IndexMap::new();
            let mut order = Vec::new();
            for (i, own) in self.checks.iter().enumerate() {
                let desc = own.desc;
                let (DataType::Object, Some(restrictions)) = (desc.data_type, &desc.object_restrictions) else {
                    continue;
                };
                let fields = restrictions
                    .values
                    .iter()
                    .map(|(key, field)| {
                        let entry = outer.entry(key.as_str());
                        let place = entry.index();
                        let groups = entry.or_default();
                        groups.push((i, field, own.depth + 1));
                        (place, groups.len() - 1)
                    })
                    .collect();
                order.push((i, fields));
            }
            let mut slots = 0;
            let places = outer
                .into_iter()
                .map(|(key, outer)| {
                    let inner = Inner::new(outer, schemas);
                    let first = slots;
                    slots += inner.groups.len();
                    (key, (inner, first))
                })
                .collect();
            Fields {
                places,
                slots,
                order,
                read: Cell::default(),
            }
        })
    }
}

/// The first mismatch of `steps`, given the results of the checks of their
/// place: the one `check_value` returns.
fn first_mismatch(steps: &[Step], results: &[Option<Message>]) -> Option<Message> {
    steps.iter().find_map(|step| match step {
        Step::Check(i) => results[*i].clone(),
        Step::Fail(e) => Some(e.clone()),
    })
}

/// The type of a value as `check_value` tells them apart, with the text of
/// a string.
#[derive(Clone, Copy)]
enum Found<'a> {
    Null,
    Boolean,
    Number,
    String(&'a str),
    Array,
    Object,
}

impl Found<'_> {
    /// The JSON type name, as `check_value` names it.
    fn name(self) -> &'static str {
        match self {
            Found::Null => "null",
            Found::Boolean => "boolean",
            Found::Number => "number",
            Found::String(_) => "string",
            Found::Array => "array",
            Found::Object => "object",
        }
    }
}

/// `check_value` of a description's own type on a value of type `found`,
/// up to the items of an array and the fields of an object, which [`Check`]
/// checks after it: the same mismatches with the same messages.
fn check_found(desc: &ValueDescription, found: Found<'_>) -> Result<(), Message> {
    let mismatch = |expected: &str| {
        Message::new("ifcx-value-type")
            .text("expected", expected)
            .text("found", found.name())
    };
    let expect = |ok: bool, expected: &str| if ok { Ok(()) } else { Err(mismatch(expected)) };
    match desc.data_type {
        DataType::Boolean => expect(matches!(found, Found::Boolean), "boolean"),
        DataType::String | DataType::DateTime | DataType::Reference => {
            expect(matches!(found, Found::String(_)), "string")
        }
        DataType::Integer | DataType::Real => expect(matches!(found, Found::Number), "number"),
        DataType::Enum => {
            let Found::String(text) = found else {
                return Err(mismatch("string"));
            };
            let options = desc
                .enum_restrictions
                .as_ref()
                .map(|r| r.options.as_slice())
                .unwrap_or_default();
            if options.iter().filter(|o| *o == text).count() == 1 {
                Ok(())
            } else {
                Err(Message::new("ifcx-value-not-option")
                    .text("value", text)
                    .text("options", options.join(", ")))
            }
        }
        DataType::Object => expect(matches!(found, Found::Object), "object"),
        DataType::Array => {
            expect(matches!(found, Found::Array), "array")?;
            match desc.array_restrictions {
                Some(_) => Ok(()),
                None => Err(Message::new("ifcx-schema-array-restrictions")),
            }
        }
        DataType::Blob => Err(Message::new("ifcx-schema-blob")),
    }
}

/// Reads one value as a [`Value`](serde_json::Value) reads it and writes
/// the result of each check of `plan` to `out`: `None` when it matches.
struct Check<'p, 's> {
    plan: &'p Plan<'s>,
    schemas: &'s SchemaSet,
    out: &'p mut [Option<Message>],
}

impl Check<'_, '_> {
    /// The results of a value of type `found` that has no items or fields.
    fn found(self, found: Found<'_>) {
        for (own, out) in self.plan.checks.iter().zip(self.out.iter_mut()) {
            *out = check_found(own.desc, found).err();
        }
    }
}

impl<'de> DeserializeSeed<'de> for Check<'_, '_> {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Check<'_, '_> {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("any valid JSON value")
    }

    fn visit_bool<E>(self, _: bool) -> Result<(), E> {
        self.found(Found::Boolean);
        Ok(())
    }

    fn visit_i64<E>(self, _: i64) -> Result<(), E> {
        self.found(Found::Number);
        Ok(())
    }

    fn visit_u64<E>(self, _: u64) -> Result<(), E> {
        self.found(Found::Number);
        Ok(())
    }

    fn visit_f64<E>(self, _: f64) -> Result<(), E> {
        self.found(Found::Number);
        Ok(())
    }

    fn visit_str<E>(self, text: &str) -> Result<(), E> {
        self.found(Found::String(text));
        Ok(())
    }

    fn visit_unit<E>(self) -> Result<(), E> {
        self.found(Found::Null);
        Ok(())
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        let Check { plan, schemas, out } = self;
        for (own, out) in plan.checks.iter().zip(out.iter_mut()) {
            *out = check_found(own.desc, Found::Array).err();
        }
        let items = plan.items(schemas);
        // The array checks without a mismatch yet, whose results are still
        // `None`: an array check fails with the first mismatch of an item.
        let mut open = items.groups.len();
        if open > 0 {
            let mut results = items.lend_results();
            let mut ended = false;
            while open > 0 {
                let item = Check {
                    plan: &items.plan,
                    schemas,
                    out: &mut results,
                };
                if seq.next_element_seed(item)?.is_none() {
                    ended = true;
                    break;
                }
                for (outer, steps) in &items.groups {
                    if out[*outer].is_none()
                        && let Some(mismatch) = first_mismatch(steps, &results)
                    {
                        out[*outer] = Some(mismatch);
                        open -= 1;
                    }
                }
            }
            items.results.set(results);
            if ended {
                return Ok(());
            }
        }
        while seq.next_element_seed(Skip)?.is_some() {}
        Ok(())
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        let mut key = map.next_key_seed(Key)?;
        if key.as_deref() == Some(RAW_VALUE_TOKEN) {
            return read_raw_value_token(&mut map, self);
        }
        let Check { plan, schemas, out } = self;
        for (own, out) in plan.checks.iter().zip(out.iter_mut()) {
            *out = check_found(own.desc, Found::Object).err();
        }
        let fields = plan.fields(schemas);
        if fields.order.is_empty() {
            while key.is_some() {
                map.next_value_seed(Skip)?;
                key = map.next_key_seed(Key)?;
            }
            return Ok(());
        }
        // The first mismatch of each group in the last field of its key,
        // `None` until the field is read: in a tree, a later field of the
        // same name replaces an earlier one.
        let mut read = fields.read.take();
        read.clear();
        read.resize(fields.slots, None);
        while let Some(name) = key {
            match fields.places.get(name.as_ref()) {
                Some((inner, first)) => {
                    let mut results = inner.lend_results();
                    let value = Check {
                        plan: &inner.plan,
                        schemas,
                        out: &mut results,
                    };
                    map.next_value_seed(value)?;
                    for (group, (_, steps)) in inner.groups.iter().enumerate() {
                        read[first + group] = Some(first_mismatch(steps, &results));
                    }
                    inner.results.set(results);
                }
                None => map.next_value_seed(Skip)?,
            }
            key = map.next_key_seed(Key)?;
        }
        // Each object check fails with the first mismatch of its fields, in
        // the order of its restrictions.
        for (outer, places) in &fields.order {
            let Some(restrictions) = &plan.checks[*outer].desc.object_restrictions else {
                continue;
            };
            for ((key, field), &(place, group)) in restrictions.values.iter().zip(places) {
                let mismatch = match &read[fields.places[place].1 + group] {
                    None if field.optional == Some(true) => None,
                    None => Some(Message::new("ifcx-value-key-missing").text("key", key.clone())),
                    Some(Some(e)) => Some(
                        Message::new("ifcx-value-in-key")
                            .text("key", key.clone())
                            .nested("reason", e.clone()),
                    ),
                    Some(None) => None,
                };
                if mismatch.is_some() {
                    out[*outer] = mismatch;
                    break;
                }
            }
        }
        fields.read.set(read);
        Ok(())
    }
}
