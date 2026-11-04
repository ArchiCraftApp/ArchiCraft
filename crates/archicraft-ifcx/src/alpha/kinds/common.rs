//! What the kind codecs share, each with its schema, writing and reading
//! side: plan points, the top link of vertical kinds (ELM-006), the wall
//! reference line (WAL-003), the drafting view of 2D elements (ANN-002) and
//! the repair of a material reference (PRJ-008).

#[cfg(feature = "test-kinds")]
use archicraft_model::DraftingView;
use archicraft_model::glam::DVec2;
use archicraft_model::{MaterialId, ReferenceLine, StoreyId, TopLink};
use serde_json::{Map, Value, json};

use super::ReadContext;
use crate::alpha::reader::{Object, Report, about, integer, real, text, uuid};
use crate::alpha::schema;
use crate::message::Message;

pub(crate) fn top_link_schema() -> Value {
    schema::object(&[
        ("link", schema::enumeration(["fixed", "storey", "relative"])),
        ("value", schema::length()),
        ("storey", schema::optional(schema::reference())),
        ("relative", schema::optional(schema::integer())),
    ])
}

pub(crate) fn reference_line_schema() -> Value {
    schema::enumeration(["outside", "center", "inside", "custom"])
}

// Only the test kinds have a drafting view so far, so the view's helpers are
// compiled with them; the first release kind with a view drops the cfg.

/// The drafting view of a 2D element (ANN-002): its type and, for a plan
/// view, the storey.
#[cfg(feature = "test-kinds")]
pub(crate) fn drafting_view_schema() -> Value {
    schema::object(&[("type", schema::enumeration(["plan"])), ("storey", schema::reference())])
}

/// The view in the model's own form, `{"type": "plan", "storey": "<uuid>"}`.
#[cfg(feature = "test-kinds")]
pub(crate) fn drafting_view_value(view: DraftingView) -> Value {
    json!(view)
}

/// The view from its members. Whether it belongs to the element's home
/// storey is checked with the element (ANN-002).
#[cfg(feature = "test-kinds")]
pub(crate) fn read_drafting_view(map: &Object) -> Result<DraftingView, Message> {
    match text(map, "type")?.as_str() {
        "plan" => Ok(DraftingView::plan(StoreyId::from_uuid(uuid(map, "storey")?))),
        other => Err(about("ifcx-field-unknown-value", "type").text("value", other)),
    }
}

pub(crate) fn point2_value(p: DVec2) -> Value {
    json!([p.x, p.y])
}

pub(crate) fn top_link_value(top: TopLink) -> Value {
    match top {
        TopLink::Fixed { height } => json!({"link": "fixed", "value": height}),
        TopLink::Storey { storey, offset } => json!({"link": "storey", "value": offset, "storey": storey.to_string()}),
        TopLink::Relative { storeys, offset } => json!({"link": "relative", "value": offset, "relative": storeys}),
    }
}

/// Adds `referenceLine`, and `referenceOffset` for a custom line.
pub(crate) fn write_reference_line(map: &mut Map<String, Value>, line: ReferenceLine) {
    let code = match line {
        ReferenceLine::Outside => "outside",
        ReferenceLine::Center => "center",
        ReferenceLine::Inside => "inside",
        ReferenceLine::Custom(_) => "custom",
    };
    map.insert("referenceLine".into(), json!(code));
    if let ReferenceLine::Custom(offset) = line {
        map.insert("referenceOffset".into(), json!(offset));
    }
}

pub(crate) fn read_top_link(map: &Object) -> Result<TopLink, Message> {
    let value = real(map, "value")?;
    match text(map, "link")?.as_str() {
        "fixed" => Ok(TopLink::Fixed { height: value }),
        "storey" => Ok(TopLink::Storey {
            storey: StoreyId::from_uuid(uuid(map, "storey")?),
            offset: value,
        }),
        "relative" => Ok(TopLink::Relative {
            storeys: i32::try_from(integer(map, "relative")?)
                .map_err(|_| about("ifcx-field-out-of-range", "relative"))?,
            offset: value,
        }),
        other => Err(about("ifcx-field-unknown-value", "link").text("value", other)),
    }
}

pub(crate) fn read_reference_line(map: &Object) -> Result<ReferenceLine, Message> {
    match text(map, "referenceLine")?.as_str() {
        "outside" => Ok(ReferenceLine::Outside),
        "center" => Ok(ReferenceLine::Center),
        "inside" => Ok(ReferenceLine::Inside),
        "custom" => Ok(ReferenceLine::Custom(real(map, "referenceOffset")?)),
        other => Err(about("ifcx-field-unknown-value", "referenceLine").text("value", other)),
    }
}

/// The element's `material`: the project's first material, reported, when
/// the reference is invalid or names no material (PRJ-008).
pub(crate) fn read_material(attr: &Object, context: &ReadContext<'_>, report: &mut Report<'_>) -> MaterialId {
    let project = context.project;
    let id = report.or(
        uuid(attr, "material").map(MaterialId::from_uuid),
        project.materials[0].id,
    );
    if project.material(id).is_some() {
        id
    } else {
        report.repaired(Message::new("ifcx-element-material-missing").text("name", project.materials[0].name.clone()));
        project.materials[0].id
    }
}
