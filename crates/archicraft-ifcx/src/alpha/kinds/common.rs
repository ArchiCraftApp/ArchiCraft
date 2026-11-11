//! What the kind codecs share, each with its schema, writing and reading
//! side: plan points, outlines with holes (SLB-001, SPC-001), the top link
//! of vertical kinds (ELM-006), the wall reference line (WAL-003), the
//! drafting view of 2D elements (ANN-002) and the repair of a material or
//! text style reference (PRJ-008).

use archicraft_model::glam::DVec2;
use archicraft_model::outline::Outline;
use archicraft_model::{DraftingView, MaterialId, Project, ReferenceLine, StoreyId, TextStyleId, TopLink};
use serde_json::{Map, Value, json};

use super::ReadContext;
use crate::alpha::reader::{Object, Report, about, array, field, integer, points, real, text, uuid};
use crate::alpha::schema;
use crate::message::Message;

pub(crate) fn top_link_schema() -> Value {
    schema::object(&[
        ("link", schema::enumeration(TopLink::CODES)),
        ("value", schema::length()),
        ("storey", schema::optional(schema::reference())),
        ("relative", schema::optional(schema::integer())),
    ])
}

pub(crate) fn reference_line_schema() -> Value {
    schema::enumeration(ReferenceLine::ALL.map(ReferenceLine::code))
}

/// The drafting view of a 2D element (ANN-002): its type and, for a plan
/// view, the storey.
pub(crate) fn drafting_view_schema() -> Value {
    schema::object(&[("type", schema::enumeration(["plan"])), ("storey", schema::reference())])
}

/// The view in the model's own form, `{"type": "plan", "storey": "<uuid>"}`.
pub(crate) fn drafting_view_value(view: DraftingView) -> Value {
    json!(view)
}

/// The view from its members. Whether it belongs to the element's home
/// storey is checked with the element (ANN-002).
pub(crate) fn read_drafting_view(map: &Object) -> Result<DraftingView, Message> {
    match text(map, "type")?.as_str() {
        "plan" => Ok(DraftingView::plan(StoreyId::from_uuid(uuid(map, "storey")?))),
        other => Err(about("ifcx-field-unknown-value", "type").text("value", other)),
    }
}

pub(crate) fn point2_value(p: DVec2) -> Value {
    json!([p.x, p.y])
}

/// The members of an outline with holes, a slab's or a space's boundary:
/// ring 0 as `outline`, the other rings as `holes`, each a list of plan
/// points.
pub(crate) fn outline_schema() -> [(&'static str, Value); 2] {
    [
        ("outline", schema::array(schema::point2())),
        ("holes", schema::array(schema::array(schema::point2()))),
    ]
}

fn ring_value(ring: &[DVec2]) -> Value {
    ring.iter().map(|p| point2_value(*p)).collect()
}

/// Adds `outline` and `holes`, at full precision (NFR-ACC-002).
pub(crate) fn write_outline(map: &mut Map<String, Value>, outline: &Outline) {
    let (outer, holes) = outline.to_points();
    map.insert("outline".into(), ring_value(&outer));
    map.insert("holes".into(), holes.iter().map(|h| ring_value(h)).collect());
}

/// The rings as read, without orienting them: `Err` without a valid
/// `outline`; invalid `holes` are left out with a report (PRJ-008).
pub(crate) fn read_outline(attr: &Object, report: &mut Report<'_>) -> Result<Outline, Message> {
    let outline = field(attr, "outline").and_then(points)?;
    let holes: Vec<_> = match array(attr, "holes").and_then(|h| h.iter().map(points).collect()) {
        Ok(h) => h,
        Err(e) => {
            report.repaired(Message::new("ifcx-holes-left-out").nested("reason", e));
            Vec::new()
        }
    };
    Ok(Outline::from_point_rings(std::iter::once(outline).chain(holes)))
}

pub(crate) fn top_link_value(top: TopLink) -> Value {
    match top {
        TopLink::Fixed { height } => json!({"link": top.code(), "value": height}),
        TopLink::Storey { storey, offset } => {
            json!({"link": top.code(), "value": offset, "storey": storey.to_string()})
        }
        TopLink::Relative { storeys, offset } => json!({"link": top.code(), "value": offset, "relative": storeys}),
    }
}

/// Adds `referenceLine`, and `referenceOffset` for a custom line.
pub(crate) fn write_reference_line(map: &mut Map<String, Value>, line: ReferenceLine) {
    map.insert("referenceLine".into(), json!(line.code()));
    if let ReferenceLine::Custom(offset) = line {
        map.insert("referenceOffset".into(), json!(offset));
    }
}

pub(crate) fn read_top_link(map: &Object) -> Result<TopLink, Message> {
    let value = real(map, "value")?;
    match text(map, "link")?.as_str() {
        TopLink::FIXED => Ok(TopLink::Fixed { height: value }),
        TopLink::STOREY => Ok(TopLink::Storey {
            storey: StoreyId::from_uuid(uuid(map, "storey")?),
            offset: value,
        }),
        TopLink::RELATIVE => Ok(TopLink::Relative {
            storeys: i32::try_from(integer(map, "relative")?)
                .map_err(|_| about("ifcx-field-out-of-range", "relative"))?,
            offset: value,
        }),
        other => Err(about("ifcx-field-unknown-value", "link").text("value", other)),
    }
}

pub(crate) fn read_reference_line(map: &Object) -> Result<ReferenceLine, Message> {
    let code = text(map, "referenceLine")?;
    match ReferenceLine::from_code(&code, 0.0) {
        Some(ReferenceLine::Custom(_)) => Ok(ReferenceLine::Custom(real(map, "referenceOffset")?)),
        Some(line) => Ok(line),
        None => Err(about("ifcx-field-unknown-value", "referenceLine").text("value", code)),
    }
}

/// The element's `material`: the project's first material, reported, when
/// the reference is invalid or names no material (PRJ-008).
pub(crate) fn read_material(attr: &Object, context: &ReadContext<'_>, report: &mut Report<'_>) -> MaterialId {
    material_or_first(attr, context.project, "ifcx-element-material-missing", report)
}

/// The `material` of `attr`, or `project`'s first material: an unreadable
/// reference is reported as such, one that names no material of `project`
/// with the message `missing`, which names the first material (PRJ-008).
pub(crate) fn material_or_first(
    attr: &Object,
    project: &Project,
    missing: &'static str,
    report: &mut Report<'_>,
) -> MaterialId {
    let first = &project.materials[0];
    let id = report.or(uuid(attr, "material").map(MaterialId::from_uuid), first.id);
    if project.material(id).is_some() {
        id
    } else {
        report.repaired(Message::new(missing).text("name", first.name.clone()));
        first.id
    }
}

/// The text's `style` (ANN-007): the project's first text style, reported,
/// when the reference is no identity or names no style of the project
/// (PRJ-008). The reader gives every project at least one style.
pub(crate) fn read_text_style(attr: &Object, context: &ReadContext<'_>, report: &mut Report<'_>) -> TextStyleId {
    let project = context.project;
    let read = uuid(attr, "style").map(TextStyleId::from_uuid);
    if let Ok(id) = read
        && project.text_style(id).is_some()
    {
        return id;
    }
    let Some(first) = project.text_styles.first() else {
        // Unreachable for a project the reader built; the text is then
        // skipped by the model's check.
        return read.unwrap_or_else(|_| TextStyleId::from_uuid(uuid::Uuid::nil()));
    };
    report.repaired(Message::new("ifcx-text-style-missing").text("name", first.name.clone()));
    first.id
}
