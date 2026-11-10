//! Validation of element parameters and polygons (ELM-017, EDT-018, EDT-019,
//! NFR-REL-005, NFR-ACC-003).
//!
//! Errors are [`Message`]s (NFR-L10N-001): numeric limits are length or angle
//! arguments, so the application shows them in the project units.

mod crossing;
mod sweep;

use glam::DVec2;

use crate::element::{Element, TopLink};
use crate::kinds::{BoundarySource, ReferenceLine, Slab, Space, VerticalLink, Wall};
use crate::message::Message;
use crate::outline::{Outline, Ring, point_bounds};
use crate::profile::{ParametricShape, Profile, ProfileDimension, ProfileRegion, ProfileShape};
use crate::project::Project;

/// Geometric coincidence tolerance: 0.01 mm (NFR-ACC-003), defined in
/// [`crate::tolerance`] and re-exported here for existing users.
pub use crate::tolerance::MODEL_TOLERANCE;

/// Edges shorter than this are removed or reported: 0.1 mm (EDT-019).
pub const MIN_EDGE_LENGTH: f64 = 1e-4;
/// Corners sharper than this are rejected as slivers: 0.1° (EDT-019).
pub const MIN_CORNER_ANGLE_DEG: f64 = 0.1;
/// Model coordinates must stay within ±100 km of the project origin
/// (NFR-ACC-001, NFR-ACC-004).
pub const MAX_COORDINATE: f64 = 100_000.0;
/// Smallest wall or slab thickness: 1 mm.
pub const MIN_THICKNESS: f64 = 0.001;
/// Largest wall or slab thickness: 10 m.
pub const MAX_THICKNESS: f64 = 10.0;
/// Smallest wall height: 1 mm.
pub const MIN_HEIGHT: f64 = 0.001;
/// Largest wall height: 1000 m.
pub const MAX_HEIGHT: f64 = 1000.0;
/// Largest base, top or slab offset: ±1000 m.
pub const MAX_OFFSET: f64 = 1000.0;
/// Largest number of storeys a wall top may be linked across.
pub const MAX_LINKED_STOREYS: usize = 200;
/// Largest number of decimals the length and angle display may use.
pub const MAX_DECIMALS: u8 = 8;

/// Most points a slab may have in its outline and holes together (SLB-003,
/// SRS Appendix D.9). The polygon checks take O(n log n) steps (a sweep
/// line), so a slab at the limit validates in milliseconds; the limit keeps a
/// crafted file from loading one slab of millions of points (NFR-SEC-003).
pub const MAX_SLAB_POINTS: usize = 10_000;

/// Smallest dimension of a parametric profile other than a radius, and
/// the shortest edge of a profile: 0.1 mm (PRF-005, EDT-019).
pub const MIN_PROFILE_DIMENSION: f64 = MIN_EDGE_LENGTH;
/// Largest dimension of a profile: of each dimension of a parametric shape
/// and of the width and height of custom regions together. Also the largest
/// distance of a profile's origin from its zero: 10 m (PRF-004, PRF-005).
pub const MAX_PROFILE_DIMENSION: f64 = MAX_THICKNESS;
/// Most regions a custom profile may have (PRF-004). Each pair of regions
/// whose boxes overlap is swept for crossing edges and, if none cross
/// deeply, intersected, both in time growing with the points of the two, so
/// the overlap check grows with the regions times the points: a profile of
/// [`MAX_SLAB_POINTS`] points and this many regions validates in tens of
/// milliseconds however they lie, where thousands of regions took seconds
/// (NFR-SEC-003).
pub const MAX_PROFILE_REGIONS: usize = 32;

/// Largest step or spacing of a drafting grid, in metres (INP-013).
pub const MAX_GRID_SPACING: f64 = 1000.0;

/// Whether both components of a grid step or spacing are finite, positive and
/// at most [`MAX_GRID_SPACING`] (INP-013).
pub fn valid_grid_spacing(spacing: DVec2) -> bool {
    spacing.is_finite() && spacing.cmpgt(DVec2::ZERO).all() && spacing.cmple(DVec2::splat(MAX_GRID_SPACING)).all()
}

/// Whether a drafting grid origin lies within [`MAX_COORDINATE`] (INP-013).
pub fn valid_grid_origin(origin: DVec2) -> bool {
    origin.is_finite() && origin.abs().cmple(DVec2::splat(MAX_COORDINATE)).all()
}

/// Removes consecutive duplicates (closer than [`MIN_EDGE_LENGTH`]),
/// including a repeated first point at the end.
pub fn clean_ring(ring: &[DVec2]) -> Vec<DVec2> {
    let mut out: Vec<DVec2> = Vec::with_capacity(ring.len());
    for &p in ring {
        if out.last().is_none_or(|q| q.distance(p) >= MIN_EDGE_LENGTH) {
            out.push(p);
        }
    }
    while out.len() > 1 && out[0].distance(out[out.len() - 1]) < MIN_EDGE_LENGTH {
        out.pop();
    }
    out
}

fn orient(a: DVec2, b: DVec2, c: DVec2) -> f64 {
    (b - a).perp_dot(c - a)
}

fn on_segment(a: DVec2, b: DVec2, p: DVec2) -> bool {
    p.x >= a.x.min(b.x) - MODEL_TOLERANCE
        && p.x <= a.x.max(b.x) + MODEL_TOLERANCE
        && p.y >= a.y.min(b.y) - MODEL_TOLERANCE
        && p.y <= a.y.max(b.y) + MODEL_TOLERANCE
}

fn point_segment_distance(p: DVec2, a: DVec2, b: DVec2) -> f64 {
    let ab = b - a;
    let len2 = ab.length_squared();
    if len2 == 0.0 {
        return p.distance(a);
    }
    let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

/// True if the closed segments `ab` and `cd` intersect or touch within the
/// model tolerance. Segments cross only where their bounding boxes overlap:
/// for collinear segments, as on a straight wall turned by 30°, the signs of
/// the orientations are rounding noise (EDT-018).
pub fn segments_touch(a: DVec2, b: DVec2, c: DVec2, d: DVec2) -> bool {
    let d1 = orient(c, d, a);
    let d2 = orient(c, d, b);
    let d3 = orient(a, b, c);
    let d4 = orient(a, b, d);
    let boxes_overlap = a.min(b).cmple(c.max(d)).all() && c.min(d).cmple(a.max(b)).all();
    if boxes_overlap
        && ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0))
        && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
    {
        return true;
    }
    point_segment_distance(a, c, d) < MODEL_TOLERANCE && on_segment(c, d, a)
        || point_segment_distance(b, c, d) < MODEL_TOLERANCE && on_segment(c, d, b)
        || point_segment_distance(c, a, b) < MODEL_TOLERANCE && on_segment(a, b, c)
        || point_segment_distance(d, a, b) < MODEL_TOLERANCE && on_segment(a, b, d)
}

fn check_coordinates(points: &[DVec2]) -> Result<(), Message> {
    if points
        .iter()
        .any(|p| !p.is_finite() || p.x.abs() > MAX_COORDINATE || p.y.abs() > MAX_COORDINATE)
    {
        return Err(Message::new("model-error-coordinates-range").length("max", MAX_COORDINATE));
    }
    Ok(())
}

/// Cleans and validates a closed ring (EDT-018). Returns the cleaned ring,
/// counter-clockwise.
pub fn validate_ring(ring: &[DVec2]) -> Result<Vec<DVec2>, Message> {
    let ring = prepare_ring(ring)?;
    // Edges that do not follow each other must not touch (NFR-SEC-003).
    if sweep::nesting(&sweep::Rings::new(&[&ring])).is_err() {
        return Err(Message::new("model-error-polygon-self-intersection"));
    }
    Ok(ring)
}

/// [`validate_ring`] up to the test for touching edges.
fn prepare_ring(ring: &[DVec2]) -> Result<Vec<DVec2>, Message> {
    check_coordinates(ring)?;
    let mut ring = clean_ring(ring);
    if ring.len() < 3 {
        return Err(Message::new("model-error-polygon-points"));
    }
    let area = Ring::<()>::from_points(ring.iter().copied()).signed_area();
    if area.abs() < MIN_EDGE_LENGTH * MIN_EDGE_LENGTH {
        return Err(Message::new("model-error-polygon-area"));
    }
    if area < 0.0 {
        ring.reverse();
    }
    let n = ring.len();
    let min_angle = MIN_CORNER_ANGLE_DEG.to_radians();
    for i in 0..n {
        let prev = ring[(i + n - 1) % n];
        let p = ring[i];
        let next = ring[(i + 1) % n];
        let angle = (prev - p).angle_to(next - p).abs();
        if angle < min_angle {
            return Err(Message::new("model-error-polygon-sliver").angle("min", min_angle));
        }
    }
    Ok(ring)
}

/// Validates a slab outline with holes. Returns the outline counter-clockwise
/// and the holes clockwise (SLB-001, SLB-003, EDT-018). It has at most
/// [`MAX_SLAB_POINTS`] points (NFR-SEC-003).
///
/// The outline is checked first, then each hole on its own, then how they
/// lie: no hole may touch the outline or lie outside it, and no two holes may
/// touch or lie in each other. When several holes are wrong, the message
/// names touching rings before a hole outside and that before nested holes.
pub fn validate_slab_polygon(
    outline: &[DVec2],
    holes: &[Vec<DVec2>],
) -> Result<(Vec<DVec2>, Vec<Vec<DVec2>>), Message> {
    if outline.len() + holes.iter().map(Vec::len).sum::<usize>() > MAX_SLAB_POINTS {
        return Err(Message::new("model-error-slab-points").count("max", MAX_SLAB_POINTS));
    }
    let outline = validate_ring(outline)?;
    let holes = holes
        .iter()
        .map(|hole| validate_ring(hole).map_err(|e| Message::new("model-error-hole").nested("reason", e)))
        .collect::<Result<Vec<_>, _>>()?;
    if holes.is_empty() {
        return Ok((outline, holes));
    }
    let rings: Vec<&[DVec2]> = std::iter::once(&outline).chain(&holes).map(Vec::as_slice).collect();
    let rings = sweep::Rings::new(&rings);
    // Ring 0 is the outline, ring i the hole i - 1.
    let parents = sweep::nesting(&rings).map_err(|(e, f)| {
        Message::new(if rings.ring(e) == 0 || rings.ring(f) == 0 {
            "model-error-hole-outside"
        } else {
            "model-error-holes-overlap"
        })
    })?;
    // A hole around nothing lies outside the outline (or around it). If no
    // hole does, all lie in the outline, and one that lies in another hole
    // overlaps it.
    if parents[1..].contains(&None) {
        return Err(Message::new("model-error-hole-outside"));
    }
    if parents[1..].iter().any(|&parent| parent != Some(0)) {
        return Err(Message::new("model-error-holes-overlap"));
    }
    let holes = holes
        .into_iter()
        .map(|mut hole| {
            hole.reverse();
            hole
        })
        .collect();
    Ok((outline, holes))
}

/// Checks a wall or slab thickness against [`MIN_THICKNESS`] and
/// [`MAX_THICKNESS`].
pub fn check_thickness(thickness: f64) -> Result<(), Message> {
    if !thickness.is_finite() || !(MIN_THICKNESS..=MAX_THICKNESS).contains(&thickness) {
        return Err(Message::new("model-error-thickness-range")
            .length("min", MIN_THICKNESS)
            .length("max", MAX_THICKNESS));
    }
    Ok(())
}

/// Checks an offset against [`MAX_OFFSET`]; `key` names the offset in the
/// message.
pub fn check_offset(value: f64, key: &'static str) -> Result<(), Message> {
    if !value.is_finite() || value.abs() > MAX_OFFSET {
        return Err(Message::new(key).length("max", MAX_OFFSET));
    }
    Ok(())
}

/// Checks a plan point against [`MAX_COORDINATE`] (NFR-ACC-001).
pub fn check_point(point: DVec2) -> Result<(), Message> {
    check_coordinates(&[point])
}

/// Range checks for a wall (ELM-017, NFR-REL-005).
pub fn validate_wall(wall: &Wall) -> Result<(), Message> {
    check_coordinates(&[wall.start, wall.end])?;
    if wall.length() < MIN_EDGE_LENGTH {
        return Err(Message::new("model-error-wall-length").length("min", MIN_EDGE_LENGTH));
    }
    check_thickness(wall.thickness)?;
    if let ReferenceLine::Custom(offset) = wall.reference_line
        && (!offset.is_finite() || offset.abs() > MAX_THICKNESS)
    {
        return Err(Message::new("model-error-reference-line-range").length("max", MAX_THICKNESS));
    }
    check_vertical_link(&wall.link())
}

/// Range checks for the base offset and top of a vertical element (ELM-006).
/// A storey the top is linked to is checked by the kind, and the resulting
/// extent by [`validate_element`]. The fixed-height message names walls, the
/// only vertical kind with a tool so far.
pub fn check_vertical_link(link: &VerticalLink) -> Result<(), Message> {
    check_offset(link.base_offset, "model-error-base-offset-range")?;
    match link.top {
        TopLink::Fixed { height } => {
            if !height.is_finite() || !(MIN_HEIGHT..=MAX_HEIGHT).contains(&height) {
                return Err(Message::new("model-error-wall-height-range")
                    .length("min", MIN_HEIGHT)
                    .length("max", MAX_HEIGHT));
            }
        }
        TopLink::Storey { offset, .. } => check_offset(offset, "model-error-top-offset-range")?,
        TopLink::Relative { storeys, offset } => {
            if storeys.unsigned_abs() as usize > MAX_LINKED_STOREYS {
                return Err(Message::new("model-error-linked-storey-range").count("max", MAX_LINKED_STOREYS));
            }
            check_offset(offset, "model-error-top-offset-range")?;
        }
    }
    Ok(())
}

/// Range checks for a slab, returning the slab with a normalised polygon:
/// its outline's rings validated by [`validate_slab_polygon`], ring 0 as the
/// outline and the others as holes.
pub fn validate_slab(slab: &Slab) -> Result<Slab, Message> {
    check_thickness(slab.thickness)?;
    check_offset(slab.offset, "model-error-slab-offset-range")?;
    let (outer, holes) = slab.outline.to_points();
    let (outer, holes) = validate_slab_polygon(&outer, &holes)?;
    Ok(Slab {
        outline: Outline::from_point_rings(std::iter::once(outer).chain(holes)),
        ..slab.clone()
    })
}

/// Range checks for a space (SPC-001), returning the space with a
/// normalised boundary: its rings validated like a slab's outline by
/// [`validate_slab_polygon`], ring 0 as the outer ring and the others as
/// holes. Its height must be positive, which [`validate_element`] checks
/// against the storeys.
pub fn validate_space(space: &Space) -> Result<Space, Message> {
    if let TopLink::Fixed { height } = space.link.top
        && !(height.is_finite() && (MIN_HEIGHT..=MAX_HEIGHT).contains(&height))
    {
        return Err(Message::new("model-error-space-height-range")
            .length("min", MIN_HEIGHT)
            .length("max", MAX_HEIGHT));
    }
    check_vertical_link(&space.link)?;
    check_point(space.stamp_offset)?;
    if let BoundarySource::Detected { seed, .. } = space.source {
        check_point(seed)?;
    }
    if space.boundary.node_count() > MAX_SLAB_POINTS {
        return Err(Message::new("model-error-space-points").count("max", MAX_SLAB_POINTS));
    }
    let (outer, holes) = space.boundary.to_points();
    let (outer, holes) = validate_slab_polygon(&outer, &holes)?;
    Ok(Space {
        boundary: Outline::from_point_rings(std::iter::once(outer).chain(holes)),
        ..space.clone()
    })
}

/// Validates a profile against the project (PRF-004, PRF-005) and returns
/// it normalised: the rings of custom regions as [`validate_slab_polygon`]
/// returns them, outer rings counter-clockwise and holes clockwise.
///
/// The name must not be blank, the origin within
/// [`MAX_PROFILE_DIMENSION`] of zero and every material must exist. A
/// parametric shape's dimensions must lie in their ranges
/// ([`MIN_PROFILE_DIMENSION`]..=[`MAX_PROFILE_DIMENSION`], radii from zero)
/// and fit together ([`check_parametric_shape`]). A custom profile has one
/// to [`MAX_PROFILE_REGIONS`] regions, at most [`MAX_SLAB_POINTS`] points
/// in all, is at most [`MAX_PROFILE_DIMENSION`] wide and tall, and its
/// regions do not overlap; they may touch, for example share an edge.
pub fn validate_profile(project: &Project, profile: &Profile) -> Result<Profile, Message> {
    if profile.name.trim().is_empty() {
        return Err(Message::new("model-error-profile-name"));
    }
    if !profile.origin.is_finite() || profile.origin.length() > MAX_PROFILE_DIMENSION {
        return Err(Message::new("model-error-profile-origin").length("max", MAX_PROFILE_DIMENSION));
    }
    if profile.materials().iter().any(|&m| project.material(m).is_none()) {
        return Err(Message::new("model-error-profile-material-missing"));
    }
    let shape = match &profile.shape {
        ProfileShape::Parametric { shape, material } => {
            check_parametric_shape(shape)?;
            ProfileShape::Parametric {
                shape: *shape,
                material: *material,
            }
        }
        ProfileShape::Custom { regions } => ProfileShape::Custom {
            regions: validate_profile_regions(regions)?,
        },
    };
    Ok(Profile {
        shape,
        ..profile.clone()
    })
}

/// Checks the dimensions of a parametric shape (PRF-005): each in its
/// range, then that they fit together. Thicknesses are smaller than the
/// space they lie in (two flanges than the height, a hollow section's wall
/// than half its smaller side, a web than the width) and fillets fit
/// between the plates they join (a root radius between the web and the
/// flange tip, and between the flange and the middle or end of the web; an
/// angle's root and toe radii together within the inner face of the shorter
/// leg, the toe radius at most the thickness). The message names the
/// dimension and its limit.
pub fn check_parametric_shape(shape: &ParametricShape) -> Result<(), Message> {
    use ProfileDimension as D;
    for (dimension, value) in shape.dimensions() {
        let min = if dimension.is_radius() {
            0.0
        } else {
            MIN_PROFILE_DIMENSION
        };
        if !value.is_finite() || !(min..=MAX_PROFILE_DIMENSION).contains(&value) {
            return Err(Message::new("model-error-profile-dimension-range")
                .nested("dimension", Message::new(dimension.label_key()))
                .length("min", min)
                .length("max", MAX_PROFILE_DIMENSION));
        }
    }
    // A plate thickness below its limit, a fillet radius at most its limit.
    let below = |dimension: D, value: f64, limit: f64| {
        if value < limit {
            Ok(())
        } else {
            Err(Message::new("model-error-profile-dimension-limit")
                .nested("dimension", Message::new(dimension.label_key()))
                .length("max", limit))
        }
    };
    let at_most = |dimension: D, value: f64, limit: f64| {
        if value <= limit {
            Ok(())
        } else {
            Err(Message::new("model-error-profile-radius-limit")
                .nested("dimension", Message::new(dimension.label_key()))
                .length("max", limit))
        }
    };
    match *shape {
        ParametricShape::Rectangle { .. } | ParametricShape::Circle { .. } => Ok(()),
        ParametricShape::RectangularHollow {
            width,
            height,
            thickness,
            outer_radius,
        } => {
            let half = width.min(height) / 2.0;
            below(D::Thickness, thickness, half)?;
            at_most(D::OuterRadius, outer_radius, half)
        }
        ParametricShape::CircularHollow { diameter, thickness } => below(D::Thickness, thickness, diameter / 2.0),
        ParametricShape::IShape {
            height,
            width,
            web,
            flange,
            root_radius,
        } => {
            below(D::Flange, flange, height / 2.0)?;
            below(D::Web, web, width)?;
            at_most(
                D::RootRadius,
                root_radius,
                ((width - web) / 2.0).min(height / 2.0 - flange),
            )
        }
        ParametricShape::TShape {
            height,
            width,
            web,
            flange,
            root_radius,
        } => {
            below(D::Flange, flange, height)?;
            below(D::Web, web, width)?;
            at_most(D::RootRadius, root_radius, ((width - web) / 2.0).min(height - flange))
        }
        ParametricShape::UShape {
            height,
            width,
            web,
            flange,
            root_radius,
        } => {
            below(D::Flange, flange, height / 2.0)?;
            below(D::Web, web, width)?;
            at_most(D::RootRadius, root_radius, (width - web).min(height / 2.0 - flange))
        }
        ParametricShape::ZShape {
            height,
            width,
            web,
            flange,
        } => {
            below(D::Flange, flange, height / 2.0)?;
            below(D::Web, web, width)
        }
        ParametricShape::LShape {
            height,
            width,
            thickness,
            root_radius,
            toe_radius,
        } => {
            let leg = height.min(width);
            below(D::Thickness, thickness, leg)?;
            at_most(D::RootRadius, root_radius, leg - thickness)?;
            // Both fillets lie on the inner face of a leg, leg - t long.
            at_most(D::ToeRadius, toe_radius, thickness.min(leg - thickness - root_radius))
        }
    }
}

/// The regions of a custom profile, each validated by
/// [`validate_slab_polygon`], checked for their size together and against
/// each other (PRF-004).
fn validate_profile_regions(regions: &[ProfileRegion]) -> Result<Vec<ProfileRegion>, Message> {
    if regions.is_empty() {
        return Err(Message::new("model-error-profile-no-regions"));
    }
    if regions.len() > MAX_PROFILE_REGIONS {
        return Err(Message::new("model-error-profile-regions").count("max", MAX_PROFILE_REGIONS));
    }
    let points: usize = regions
        .iter()
        .map(|r| r.outer.len() + r.holes.iter().map(Vec::len).sum::<usize>())
        .sum();
    if points > MAX_SLAB_POINTS {
        return Err(Message::new("model-error-profile-points").count("max", MAX_SLAB_POINTS));
    }
    let regions = regions
        .iter()
        .enumerate()
        .map(|(i, region)| {
            let (outer, holes) = validate_slab_polygon(&region.outer, &region.holes).map_err(|e| {
                Message::new("model-error-profile-region")
                    .count("number", i + 1)
                    .nested("reason", e)
            })?;
            Ok(ProfileRegion {
                outer,
                holes,
                material: region.material,
            })
        })
        .collect::<Result<Vec<_>, Message>>()?;
    let areas: Vec<RegionArea> = regions.iter().map(RegionArea::new).collect();
    let (min, max) = point_bounds(areas.iter().flat_map(|a| [a.bounds.0, a.bounds.1])).unwrap_or_default();
    if (max - min).max_element() > MAX_PROFILE_DIMENSION {
        return Err(Message::new("model-error-profile-size").length("max", MAX_PROFILE_DIMENSION));
    }
    for (i, a) in areas.iter().enumerate() {
        for (j, b) in areas.iter().enumerate().skip(i + 1) {
            if a.overlaps(b) {
                return Err(Message::new("model-error-profile-regions-overlap")
                    .count("first", i + 1)
                    .count("second", j + 1));
            }
        }
    }
    Ok(regions)
}

/// A valid profile region prepared for the overlap test, once per region
/// rather than once per pair (NFR-SEC-003).
struct RegionArea<'a> {
    region: &'a ProfileRegion,
    polygon: geo::Polygon<f64>,
    bounds: (DVec2, DVec2),
    /// The length of the outer ring.
    boundary: f64,
}

impl<'a> RegionArea<'a> {
    fn new(region: &'a ProfileRegion) -> Self {
        use geo::{Euclidean, Length};
        let outline = Outline::<()>::from_point_rings(
            std::iter::once(&region.outer)
                .chain(&region.holes)
                .map(|ring| ring.iter().copied()),
        );
        let polygon = outline.to_geo();
        let boundary = Euclidean.length(polygon.exterior());
        Self {
            // A valid region has at least three points.
            bounds: outline.bounds().unwrap_or_default(),
            region,
            polygon,
            boundary,
        }
    }

    /// Whether two regions overlap (PRF-004): their edges cross deeply,
    /// each passing more than [`MODEL_TOLERANCE`] to either side of the
    /// other ([`crossing::cross_deeply`]), or their intersection is more
    /// than a strip `MODEL_TOLERANCE` wide along the shorter boundary.
    /// Regions that only touch, sharing an edge or a point or lying in the
    /// other's hole, do not overlap. The sweep for crossings comes first:
    /// without deep crossings, the intersection has about as many points as
    /// the regions, where crossed combs give it millions (NFR-SEC-003).
    fn overlaps(&self, other: &RegionArea) -> bool {
        use geo::{Area, BooleanOps};
        let ((a_min, a_max), (b_min, b_max)) = (self.bounds, other.bounds);
        if a_max.cmplt(b_min).any() || b_max.cmplt(a_min).any() {
            return false;
        }
        // Edges cross inside both boxes, so only edges reaching into both
        // can cross.
        let (min, max) = (a_min.max(b_min), a_max.min(b_max));
        if crossing::cross_deeply(self.edges_within(min, max), other.edges_within(min, max)) {
            return true;
        }
        let strip = MODEL_TOLERANCE * self.boundary.min(other.boundary);
        self.polygon.intersection(&other.polygon).unsigned_area() > strip
    }

    /// The edges of the region's rings whose boxes reach into the box from
    /// `min` to `max`.
    fn edges_within(&self, min: DVec2, max: DVec2) -> impl Iterator<Item = (DVec2, DVec2)> + '_ {
        std::iter::once(&self.region.outer)
            .chain(&self.region.holes)
            .flat_map(|ring| (0..ring.len()).map(move |i| (ring[i], ring[(i + 1) % ring.len()])))
            .filter(move |&(a, b)| a.max(b).cmpge(min).all() && a.min(b).cmple(max).all())
    }
}

/// Validates an element against the project it is placed in: references to
/// storey, layer, material, text style and profiles (PRF-004), its
/// parameters, what placing it adds to its tool's defaults
/// ([`crate::kinds::Kind::check_complete`]) and its vertical extent. On
/// success, returns the element with normalised geometry. An element without
/// a body has no height to check: its extent is flat by design (ANN-002).
pub fn validate_element(project: &Project, element: &Element) -> Result<Element, Message> {
    let element = validate_prototype(project, element)?;
    element.kind.check_complete()?;
    match project.vertical_extent(&element) {
        Some((bottom, top)) if top - bottom < MIN_HEIGHT && element.kind.descriptor().has_body => {
            Err(Message::new("model-error-top-below-base"))
        }
        _ => Ok(element),
    }
}

/// [`validate_element`] for a tool's prototype (ELM-001): everything but the
/// height, which depends on the storey the tool draws on, and what the user
/// adds when placing the element, such as a text's content
/// ([`crate::kinds::Kind::check_complete`]). The extent must be
/// determinable, so linked storeys must exist (ELM-006).
pub fn validate_prototype(project: &Project, element: &Element) -> Result<Element, Message> {
    if element.name.trim().is_empty() {
        return Err(Message::new("model-error-element-name"));
    }
    if project.storeys.get(element.storey).is_none() {
        return Err(Message::new("model-error-home-storey-missing"));
    }
    if project.layer(element.layer).is_none() {
        return Err(Message::new("model-error-element-layer-missing"));
    }
    if element.kind.material().is_some_and(|m| project.material(m).is_none()) {
        return Err(Message::new("model-error-element-material-missing"));
    }
    if element
        .kind
        .text_style()
        .is_some_and(|s| project.text_style(s).is_none())
    {
        return Err(Message::new("model-error-text-style-missing"));
    }
    // PRF-004: every profile exists and allows the element's kind.
    for id in element.kind.profiles() {
        let Some(profile) = project.profile(id) else {
            return Err(Message::new("model-error-element-profile-missing"));
        };
        if !element.kind.accepts_profile(profile.usage) {
            return Err(Message::new("model-error-profile-usage")
                .text("name", &profile.name)
                .kind("kind", element.tag()));
        }
    }
    // ELM-011: the class must be one of the kind's selectable classes.
    let descriptor = element.kind.descriptor();
    if !descriptor.ifc_classes.contains(&element.ifc_class) {
        return Err(Message::new("model-error-ifc-class-not-allowed")
            .text("class", element.ifc_class.code())
            .kind("kind", element.tag()));
    }
    // IFX-024: the type must be one the kind offers with the class.
    if let Some(predefined) = element.predefined_type
        && !element.tag().predefined_types(element.ifc_class).contains(&predefined)
    {
        return Err(Message::new("model-error-predefined-type-not-allowed")
            .nested("type", Message::new(predefined.label_key()))
            .text("class", element.ifc_class.code()));
    }
    let element = Element {
        kind: element.kind.validate(element.storey, &project.storeys)?,
        ..element.clone()
    };
    if project.vertical_extent(&element).is_none() {
        return Err(Message::new("model-error-vertical-extent"));
    }
    Ok(element)
}

#[cfg(test)]
mod tests {
    use std::f64::consts::TAU;
    use std::time::{Duration, Instant};

    use super::*;

    /// The signed area of a ring of points, positive counter-clockwise.
    fn signed_area(ring: &[DVec2]) -> f64 {
        Ring::<()>::from_points(ring.iter().copied()).signed_area()
    }

    /// Whether `p` lies inside the ring of points (even-odd rule).
    fn point_in_ring(p: DVec2, ring: &[DVec2]) -> bool {
        Ring::<()>::from_points(ring.iter().copied()).contains(p)
    }

    fn square(size: f64) -> Vec<DVec2> {
        vec![
            DVec2::new(0.0, 0.0),
            DVec2::new(size, 0.0),
            DVec2::new(size, size),
            DVec2::new(0.0, size),
        ]
    }

    #[test]
    fn ring_cleanup_and_orientation() {
        let mut ring = square(1.0);
        ring.insert(1, DVec2::new(0.0, 0.0));
        ring.push(DVec2::new(0.0, 0.0));
        ring.reverse();
        let cleaned = validate_ring(&ring).unwrap();
        assert_eq!(cleaned.len(), 4);
        assert!(signed_area(&cleaned) > 0.0);
    }

    #[test]
    fn rejects_degenerate_rings() {
        assert!(validate_ring(&[DVec2::ZERO, DVec2::X]).is_err());
        assert!(validate_ring(&[DVec2::ZERO, DVec2::X, DVec2::new(2.0, 0.0)]).is_err());
        let bow_tie = vec![
            DVec2::new(0.0, 0.0),
            DVec2::new(1.0, 1.0),
            DVec2::new(1.0, 0.0),
            DVec2::new(0.0, 1.0),
        ];
        assert!(validate_ring(&bow_tie).is_err());
    }

    #[test]
    fn holes_must_be_inside() {
        let hole: Vec<DVec2> = square(1.0).into_iter().map(|p| p + DVec2::splat(1.0)).collect();
        let (outline, holes) = validate_slab_polygon(&square(4.0), &[hole]).unwrap();
        assert!(signed_area(&outline) > 0.0);
        assert!(signed_area(&holes[0]) < 0.0);
        let outside: Vec<DVec2> = square(1.0).into_iter().map(|p| p + DVec2::splat(5.0)).collect();
        assert!(validate_slab_polygon(&square(4.0), &[outside]).is_err());
        let touching: Vec<DVec2> = square(1.0);
        assert!(validate_slab_polygon(&square(4.0), &[touching]).is_err());
    }

    /// A comb with `teeth` teeth: `4 * teeth + 3` points, right angles only
    /// and no self-intersection.
    fn comb(teeth: usize) -> Vec<DVec2> {
        long_comb(teeth, 1.0)
    }

    /// [`comb`] with teeth `length` long, 1 m wide and 1 m apart.
    fn long_comb(teeth: usize, length: f64) -> Vec<DVec2> {
        let mut ring = Vec::new();
        for i in 0..teeth {
            let x = 2.0 * i as f64;
            ring.extend([
                DVec2::new(x, 0.0),
                DVec2::new(x, length),
                DVec2::new(x + 1.0, length),
                DVec2::new(x + 1.0, 0.0),
            ]);
        }
        let end = 2.0 * teeth as f64;
        ring.extend([DVec2::new(end, 0.0), DVec2::new(end, -1.0), DVec2::new(0.0, -1.0)]);
        ring
    }

    /// `ring` turned by `degrees` about the origin.
    fn turned(ring: &[DVec2], degrees: f64) -> Vec<DVec2> {
        let rotation = DVec2::from_angle(degrees.to_radians());
        ring.iter().map(|&p| rotation.rotate(p)).collect()
    }

    /// `ring` moved by `offset`.
    fn moved(ring: &[DVec2], offset: DVec2) -> Vec<DVec2> {
        ring.iter().map(|&p| p + offset).collect()
    }

    /// NFR-SEC-003, SLB-003: a slab may have up to 10,000 points in its
    /// outline and holes together (SRS Appendix D.9); a slab with more is
    /// rejected with a message before the polygon checks run.
    #[test]
    fn slab_polygons_are_limited_to_max_slab_points() {
        assert_eq!(MAX_SLAB_POINTS, 10_000);
        let largest = comb((MAX_SLAB_POINTS - 3) / 4);
        assert!(largest.len() <= MAX_SLAB_POINTS);
        assert!(validate_slab_polygon(&largest, &[]).is_ok());
        let too_large = comb((MAX_SLAB_POINTS - 3) / 4 + 1);
        assert!(too_large.len() > MAX_SLAB_POINTS);
        let error = validate_slab_polygon(&too_large, &[]).unwrap_err();
        assert_eq!(error, Message::new("model-error-slab-points").count("max", 10_000));
        // The holes count too.
        let hole = moved(&square(0.5), DVec2::new(0.25, -0.75));
        let holes = vec![hole; MAX_SLAB_POINTS / 4];
        let error = validate_slab_polygon(&comb(10), &holes).unwrap_err();
        assert_eq!(error.key(), "model-error-slab-points");
    }

    type Holes = Vec<Vec<DVec2>>;

    /// Valid slabs at the point limit that are hard for the polygon checks:
    /// the comb outline, a rectangle with 2,499 holes, a comb with holes in
    /// its base, and combs with teeth 1 km long turned by 30° and by 90°, so
    /// that thousands of long edges are side by side.
    fn slabs_at_the_limit() -> Vec<(&'static str, Vec<DVec2>, Holes)> {
        let teeth = (MAX_SLAB_POINTS - 3) / 4;
        let grid_holes: Vec<Vec<DVec2>> = (0..teeth)
            .map(|i| moved(&square(0.5), DVec2::new((i % 50) as f64, (i / 50) as f64) + 0.25))
            .collect();
        let base_holes: Vec<Vec<DVec2>> = (0..teeth / 2)
            .map(|i| moved(&square(0.5), DVec2::new(2.0 * i as f64 + 0.25, -0.75)))
            .collect();
        vec![
            ("comb", comb(teeth), Vec::new()),
            ("rectangle with holes", square(50.0), grid_holes),
            ("comb with holes", comb(teeth / 2 + 1), base_holes),
            (
                "long comb turned by 30°",
                turned(&long_comb(teeth, 1000.0), 30.0),
                Vec::new(),
            ),
            (
                "long comb turned by 90°",
                turned(&long_comb(teeth, 1000.0), 90.0),
                Vec::new(),
            ),
        ]
    }

    /// NFR-SEC-003, SLB-003: the polygon checks grow little faster than the
    /// points, not with their square, so each slab of [`slabs_at_the_limit`]
    /// validates in under 100 ms in a release build (about 10 ms; testing
    /// every pair of edges took about 0.6 s for one). Test builds, optimised
    /// less, get 250 ms.
    #[test]
    fn slabs_at_the_point_limit_validate_quickly() {
        let budget = Duration::from_millis(if cfg!(debug_assertions) { 250 } else { 100 });
        for (name, outline, holes) in slabs_at_the_limit() {
            let points = outline.len() + holes.iter().map(Vec::len).sum::<usize>();
            assert!(
                (MAX_SLAB_POINTS - 10..=MAX_SLAB_POINTS).contains(&points),
                "{name}: {points} points"
            );
            let start = Instant::now();
            let result = validate_slab_polygon(&outline, &holes);
            let elapsed = start.elapsed();
            assert!(result.is_ok(), "{name}: {result:?}");
            assert!(elapsed < budget, "{name} took {elapsed:?}");
        }
    }

    /// NFR-SEC-003: the sweep tests a few pairs of edges per point, whatever
    /// the shape: at most five for each slab of [`slabs_at_the_limit`],
    /// where testing every pair takes about 50 million.
    #[test]
    fn the_sweep_tests_a_few_pairs_of_edges_per_point() {
        for (name, outline, holes) in slabs_at_the_limit() {
            let rings: Vec<Vec<DVec2>> = std::iter::once(&outline)
                .chain(&holes)
                .map(|ring| validate_ring(ring).unwrap())
                .collect();
            let points: usize = rings.iter().map(Vec::len).sum();
            let rings: Vec<&[DVec2]> = rings.iter().map(Vec::as_slice).collect();
            let rings = sweep::Rings::new(&rings);
            assert!(sweep::by_sweep(&rings).is_ok(), "{name}");
            let tested = rings.tested.get();
            assert!(
                tested <= 5 * points,
                "{name}: {tested} pairs tested for {points} points"
            );
        }
    }

    /// A ring with a spike from the left whose tip, at the origin, points
    /// into a slot between x = -1 and the right side `right` (points listed
    /// downwards). Counter-clockwise, simple apart from what `right` brings
    /// close to the tip.
    fn spike_into_slot(right: &[DVec2]) -> Vec<DVec2> {
        let mut ring = vec![DVec2::new(-3.0, -3.0), DVec2::new(3.0, -3.0), DVec2::new(3.0, 3.0)];
        ring.extend_from_slice(right);
        ring.extend([
            DVec2::new(-1.0, -2.0),
            DVec2::new(-1.0, -0.5),
            DVec2::ZERO,
            DVec2::new(-1.0, 0.5),
            DVec2::new(-1.0, 3.0),
            DVec2::new(-3.0, 3.0),
        ]);
        ring
    }

    /// Runs the sweep and the test of every pair on `rings` (as
    /// [`prepare_ring`] leaves them), asserts that they agree on whether
    /// edges touch and, if none do, on how the rings nest, and returns
    /// whether edges touch.
    fn sweep_agrees(rings: &[Vec<DVec2>]) -> bool {
        let slices: Vec<&[DVec2]> = rings.iter().map(Vec::as_slice).collect();
        let rings = sweep::Rings::new(&slices);
        let (swept, every) = (sweep::by_sweep(&rings), sweep::every_pair(&rings));
        match (&swept, &every) {
            (Ok(a), Ok(b)) => assert_eq!(a, b, "nesting of {slices:?}"),
            _ => assert_eq!(swept.is_err(), every.is_err(), "touching edges in {slices:?}"),
        }
        swept.is_err()
    }

    /// EDT-018, NFR-ACC-003: an edge within the model tolerance of a point
    /// it does not end at is found whatever its direction: a steep edge that
    /// does not reach the vertical through the point, an edge ending next to
    /// the point that reaches neither the vertical nor the horizontal through
    /// it, and two long edges crossing far from every point. A little further
    /// away than the tolerance, the same rings are valid. The sweep finds
    /// them as testing every pair does, here and far from the origin.
    #[test]
    fn touching_edges_are_found_in_every_direction() {
        for away in [0.9, 1.1] {
            let d = away * MODEL_TOLERANCE;
            let touch = away < 1.0;
            // The slot's right side is a steep edge from (d, -2) to (d + tol/100, 3).
            let steep = spike_into_slot(&[DVec2::new(d + MODEL_TOLERANCE / 100.0, 3.0), DVec2::new(d, -2.0)]);
            // A second spike from the right ends d from the first one's tip, diagonally.
            let tip = DVec2::splat(d / 2.0_f64.sqrt());
            let tips = spike_into_slot(&[
                DVec2::new(1.0, 3.0),
                DVec2::new(1.0, 1.0),
                tip,
                DVec2::new(1.0, 0.5),
                DVec2::new(1.0, -2.0),
            ]);
            for ring in [steep, tips] {
                assert_eq!(validate_ring(&ring).is_err(), touch, "{away}: {ring:?}");
                assert_eq!(sweep_agrees(&[prepare_ring(&ring).unwrap()]), touch, "{away}: {ring:?}");
                let far = moved(&turned(&ring, 37.0), DVec2::new(-54_321.0, 76_543.0));
                sweep_agrees(&[prepare_ring(&far).unwrap()]);
            }
        }
        let crossing = [
            DVec2::new(0.0, 0.0),
            DVec2::new(10.0, 10.0),
            DVec2::new(10.0, 0.0),
            DVec2::new(0.0, 4.0),
        ];
        assert_eq!(
            validate_ring(&crossing),
            Err(Message::new("model-error-polygon-self-intersection"))
        );
        assert!(sweep_agrees(&[prepare_ring(&crossing).unwrap()]));
    }

    /// SLB-003: holes must not touch or lie in each other, whichever comes
    /// first, and a hole around the outline lies outside it.
    #[test]
    fn holes_must_not_touch_or_contain_each_other() {
        let outline = square(10.0);
        let left = moved(&square(2.0), DVec2::splat(1.0));
        for (away, valid) in [(0.9, false), (1.1, true)] {
            let right = moved(&square(2.0), DVec2::new(3.0 + away * MODEL_TOLERANCE, 1.0));
            let result = validate_slab_polygon(&outline, &[left.clone(), right]);
            assert_eq!(result.is_ok(), valid, "{away}: {result:?}");
            if !valid {
                assert_eq!(result.unwrap_err().key(), "model-error-holes-overlap");
            }
        }
        let big = moved(&square(6.0), DVec2::splat(1.0));
        let small = moved(&square(1.0), DVec2::splat(3.0));
        // The sweep finds that the small hole lies in the big one.
        let rings = [&outline, &big, &small].map(|ring| prepare_ring(ring).unwrap());
        let slices: Vec<&[DVec2]> = rings.iter().map(Vec::as_slice).collect();
        let nesting = sweep::by_sweep(&sweep::Rings::new(&slices));
        assert_eq!(nesting, Ok(vec![None, Some(0), Some(1)]));
        for holes in [[big.clone(), small.clone()], [small, big]] {
            let error = validate_slab_polygon(&outline, &holes).unwrap_err();
            assert_eq!(error.key(), "model-error-holes-overlap");
        }
        let error = validate_slab_polygon(&moved(&square(1.0), DVec2::splat(5.0)), &[outline]).unwrap_err();
        assert_eq!(error.key(), "model-error-hole-outside");
    }

    /// A small deterministic random generator (splitmix64) for the random
    /// rings below.
    struct Random(u64);

    impl Random {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            z ^ (z >> 31)
        }

        /// A number in [0, 1).
        fn unit(&mut self) -> f64 {
            (self.next() >> 11) as f64 / (1_u64 << 53) as f64
        }

        fn below(&mut self, n: usize) -> usize {
            (self.next() % n as u64) as usize
        }

        fn pick<T: Copy>(&mut self, items: &[T]) -> T {
            items[self.below(items.len())]
        }

        /// A direction at a random angle.
        fn direction(&mut self) -> DVec2 {
            DVec2::from_angle(self.unit() * TAU)
        }
    }

    /// Distances around the model tolerance, as multiples of it.
    const SPACINGS: [f64; 12] = [0.0, 0.3, 0.7, 0.9, 0.99, 0.999, 1.001, 1.01, 1.1, 1.3, 1.45, 1.6];

    /// A star of 4 to 123 points about the origin, `scale` in radius at most,
    /// valid but for the rare crossing; sometimes two of its points are
    /// swapped, which makes edges cross.
    fn star(random: &mut Random, scale: f64) -> Vec<DVec2> {
        let most = random.pick(&[8, 28, 120]);
        let n = 4 + random.below(most);
        let mut angles: Vec<f64> = (0..n).map(|_| random.unit() * TAU).collect();
        angles.sort_by(f64::total_cmp);
        let mut ring: Vec<DVec2> = angles
            .iter()
            .map(|&a| DVec2::from_angle(a) * scale * (0.2 + 0.8 * random.unit()))
            .collect();
        if random.below(4) == 0 {
            let (i, j) = (random.below(n), random.below(n));
            ring.swap(i, j);
        }
        ring
    }

    /// A [`star`] at a random scale, from 1 mm to 1 km.
    fn random_star(random: &mut Random) -> Vec<DVec2> {
        let scale = random.pick(&[0.001, 0.1, 10.0, 1000.0]);
        star(random, scale)
    }

    /// A ring with a spike whose tip lies a distance around the model
    /// tolerance from the slot's right side: a straight side leaning by a
    /// random amount (steep to shallow), or a second spike whose tip lies in
    /// a random direction from the first one's.
    fn random_spike(random: &mut Random) -> Vec<DVec2> {
        let away = random.pick(&SPACINGS) * MODEL_TOLERANCE;
        let mut ring = if random.below(2) == 0 {
            let lean = random.pick(&[0.0, MODEL_TOLERANCE / 100.0, 0.01, 0.2, 0.45]);
            let (bottom, top) = (DVec2::new(0.5 - lean, -2.0), DVec2::new(0.5 + lean, 3.0));
            let height = -1.5 + 4.0 * random.unit();
            let side = bottom.lerp(top, (height + 2.0) / 5.0);
            let into_slot = (top - bottom).perp().normalize();
            let tip = side + into_slot * away;
            let mut ring = spike_into_slot(&[top, bottom]);
            // The spike's base and tip follow the height of the tip.
            ring[6] = DVec2::new(-1.0, height - 0.5);
            ring[7] = tip;
            ring[8] = DVec2::new(-1.0, height + 0.5);
            ring
        } else {
            let mut direction = random.direction();
            direction.x = direction.x.abs();
            let tip = direction * away;
            spike_into_slot(&[
                DVec2::new(1.0, 3.0),
                DVec2::new(1.0, tip.y + 0.5 + random.unit()),
                tip,
                DVec2::new(1.0, tip.y - 0.5 - random.unit()),
                DVec2::new(1.0, -2.0),
            ])
        };
        if random.below(3) == 0 {
            ring.reverse();
        }
        ring
    }

    /// A ring of 4 to 11 points on a coarse integer grid: often crossing
    /// itself, with points on other edges, edges lying on each other and
    /// points repeated.
    fn random_grid_ring(random: &mut Random) -> Vec<DVec2> {
        let n = 4 + random.below(8);
        (0..n)
            .map(|_| DVec2::new(random.below(5) as f64, random.below(5) as f64))
            .collect()
    }

    /// `ring` turned by a random angle (often a right one) and moved, often
    /// far from the origin, where rounding is coarser.
    fn randomly_placed(random: &mut Random, ring: &[DVec2]) -> Vec<DVec2> {
        let any = random.unit() * 360.0;
        let degrees = random.pick(&[0.0, 90.0, 180.0, 30.0, 45.0, any]);
        let offset = random.pick(&[
            DVec2::ZERO,
            DVec2::new(54_321.125, -76_543.375),
            DVec2::new(-98_765.5, 87_654.25),
        ]);
        moved(&turned(ring, degrees), offset)
    }

    /// The polygon checks as they were before the sweep line
    /// (archicraft-91jj), testing every pair of edges: the reference the
    /// sweep must agree with.
    mod pairwise {
        use super::super::*;
        use super::{point_in_ring, signed_area};

        pub fn validate_ring(ring: &[DVec2]) -> Result<Vec<DVec2>, Message> {
            check_coordinates(ring)?;
            let mut ring = clean_ring(ring);
            if ring.len() < 3 {
                return Err(Message::new("model-error-polygon-points"));
            }
            let area = signed_area(&ring);
            if area.abs() < MIN_EDGE_LENGTH * MIN_EDGE_LENGTH {
                return Err(Message::new("model-error-polygon-area"));
            }
            if area < 0.0 {
                ring.reverse();
            }
            let n = ring.len();
            let min_angle = MIN_CORNER_ANGLE_DEG.to_radians();
            for i in 0..n {
                let prev = ring[(i + n - 1) % n];
                let p = ring[i];
                let next = ring[(i + 1) % n];
                let angle = (prev - p).angle_to(next - p).abs();
                if angle < min_angle {
                    return Err(Message::new("model-error-polygon-sliver").angle("min", min_angle));
                }
            }
            for i in 0..n {
                let (a, b) = (ring[i], ring[(i + 1) % n]);
                for j in (i + 1)..n {
                    if j == i + 1 || (i == 0 && j == n - 1) {
                        continue;
                    }
                    let (c, d) = (ring[j], ring[(j + 1) % n]);
                    if segments_touch(a, b, c, d) {
                        return Err(Message::new("model-error-polygon-self-intersection"));
                    }
                }
            }
            Ok(ring)
        }

        fn rings_touch(a: &[DVec2], b: &[DVec2]) -> bool {
            let (na, nb) = (a.len(), b.len());
            (0..na).any(|i| (0..nb).any(|j| segments_touch(a[i], a[(i + 1) % na], b[j], b[(j + 1) % nb])))
        }

        pub fn validate_slab_polygon(
            outline: &[DVec2],
            holes: &[Vec<DVec2>],
        ) -> Result<(Vec<DVec2>, Vec<Vec<DVec2>>), Message> {
            let outline = validate_ring(outline)?;
            let mut cleaned_holes: Vec<Vec<DVec2>> = Vec::with_capacity(holes.len());
            for hole in holes {
                let mut hole = validate_ring(hole).map_err(|e| Message::new("model-error-hole").nested("reason", e))?;
                if !point_in_ring(hole[0], &outline) || rings_touch(&hole, &outline) {
                    return Err(Message::new("model-error-hole-outside"));
                }
                if cleaned_holes.iter().any(|other| {
                    rings_touch(&hole, other) || point_in_ring(hole[0], other) || point_in_ring(other[0], &hole)
                }) {
                    return Err(Message::new("model-error-holes-overlap"));
                }
                hole.reverse();
                cleaned_holes.push(hole);
            }
            Ok((outline, cleaned_holes))
        }
    }

    /// EDT-018, NFR-SEC-003: the sweep finds touching edges exactly where
    /// testing every pair does, and rings validate as they did with every
    /// pair tested: random stars, spikes whose tips lie around the model
    /// tolerance from another edge or end point, and rings on a coarse grid
    /// that touch themselves in every way, at random angles and often far
    /// from the origin.
    #[test]
    fn rings_validate_as_with_every_pair_of_edges_tested() {
        let mut random = Random(91);
        let (mut apart, mut touching) = (0, 0);
        for case in 0..6000 {
            let ring = match case % 3 {
                0 => random_star(&mut random),
                1 => random_spike(&mut random),
                _ => random_grid_ring(&mut random),
            };
            let ring = randomly_placed(&mut random, &ring);
            assert_eq!(
                validate_ring(&ring),
                pairwise::validate_ring(&ring),
                "case {case}: {ring:?}"
            );
            if let Ok(prepared) = prepare_ring(&ring) {
                if sweep_agrees(&[prepared]) {
                    touching += 1;
                } else {
                    apart += 1;
                }
            }
        }
        // Both outcomes are common, so the comparison means something.
        assert!(apart > 1500 && touching > 1500, "{apart} apart, {touching} touching");
    }

    /// SLB-003, NFR-SEC-003: the sweep finds touching edges and nested rings
    /// as testing every pair does, and slabs validate as they did with every
    /// pair tested: an outline with up to three holes, each placed at random, next
    /// to the outline or another hole at a distance around the model
    /// tolerance, or around another hole. With one hole the message is the
    /// same; with several, a slab with two problems may name the other one.
    #[test]
    fn slabs_validate_as_with_every_pair_of_edges_tested() {
        let mut random = Random(9_100);
        let (mut valid, mut invalid, mut apart, mut touching) = (0, 0, 0, 0);
        for case in 0..3000 {
            let outline = if random.below(3) > 0 {
                square(10.0)
            } else {
                moved(&star(&mut random, 6.0), DVec2::splat(5.0))
            };
            let mut holes: Vec<Vec<DVec2>> = Vec::new();
            for _ in 0..1 + random.below(3) {
                let size = random.pick(&[0.5, 1.0, 2.0, 12.0]);
                let corner = match random.below(4) {
                    // Anywhere.
                    0 | 1 => DVec2::new(random.unit(), random.unit()) * 6.0 + 1.0,
                    // Next to an outline point.
                    2 => {
                        outline[random.below(outline.len())]
                            + random.direction() * random.pick(&SPACINGS) * MODEL_TOLERANCE
                    }
                    // Next to the last hole's right side.
                    _ => {
                        holes.last().map_or(DVec2::splat(1.0), |h: &Vec<DVec2>| h[1])
                            + DVec2::new(random.pick(&SPACINGS) * MODEL_TOLERANCE, 0.0)
                    }
                };
                holes.push(moved(&square(size), corner));
            }
            let degrees = random.pick(&[0.0, 30.0, 90.0]);
            let outline = turned(&outline, degrees);
            let holes: Vec<Vec<DVec2>> = holes.iter().map(|h| turned(h, degrees)).collect();
            let expected = pairwise::validate_slab_polygon(&outline, &holes);
            let result = validate_slab_polygon(&outline, &holes);
            if holes.len() == 1 {
                assert_eq!(result, expected, "case {case}: {outline:?} {holes:?}");
            } else {
                assert_eq!(result.is_ok(), expected.is_ok(), "case {case}: {outline:?} {holes:?}");
            }
            if expected.is_ok() {
                valid += 1;
            } else {
                invalid += 1;
            }
            let rings: Result<Vec<_>, _> = std::iter::once(&outline)
                .chain(&holes)
                .map(|r| prepare_ring(r))
                .collect();
            if let Ok(rings) = rings {
                if sweep_agrees(&rings) {
                    touching += 1;
                } else {
                    apart += 1;
                }
            }
        }
        assert!(valid > 500 && invalid > 500, "{valid} valid, {invalid} invalid");
        assert!(apart > 500 && touching > 500, "{apart} apart, {touching} touching");
    }

    mod profiles {
        use super::*;
        use crate::Arg;
        use crate::ids::MaterialId;
        use crate::profile::{ParametricShape, Profile, ProfileDimension as D, ProfileRegion, ProfileShape};
        use crate::template::default_metric_project;

        const MM: f64 = 0.001;

        /// The default shape of [`ParametricShape::ALL`] with `code`, so a
        /// reorder of `ALL` changes no test.
        fn standard(code: &str) -> ParametricShape {
            ParametricShape::ALL
                .into_iter()
                .find(|shape| shape.code() == code)
                .unwrap_or_else(|| panic!("no standard shape {code}"))
        }

        fn ipe_200() -> ParametricShape {
            ParametricShape::IShape {
                height: 200.0 * MM,
                width: 100.0 * MM,
                web: 5.6 * MM,
                flange: 8.5 * MM,
                root_radius: 12.0 * MM,
            }
        }

        /// The project and a parametric profile of `shape` made of its
        /// first material.
        fn parametric(shape: ParametricShape) -> (Project, Profile) {
            let project = default_metric_project();
            let material = project.materials[0].id;
            (project, Profile::parametric("P", shape, material))
        }

        fn check(shape: ParametricShape) -> Result<Profile, Message> {
            let (project, profile) = parametric(shape);
            validate_profile(&project, &profile)
        }

        /// `shape` with `dimension` set to `value`.
        fn with(mut shape: ParametricShape, dimension: D, value: f64) -> ParametricShape {
            *shape.dimension_mut(dimension).unwrap() = value;
            shape
        }

        /// The dimension a message names.
        fn named_dimension(message: &Message) -> &'static str {
            match message.arg("dimension") {
                Some(Arg::Nested(name)) => name.key(),
                other => panic!("{message} names no dimension: {other:?}"),
            }
        }

        /// PRF-005: every standard shape accepts typical dimensions: IPE 200
        /// (h 200, b 100, tw 5.6, tf 8.5, r 12 mm), CHS 168.3 × 8, L 100 ×
        /// 100 × 10 with r1 12 and r2 6 mm, and the defaults of each
        /// variant. A parametric profile comes back unchanged.
        #[test]
        fn parametric_shapes_accept_typical_dimensions() {
            let typical = [
                ipe_200(),
                ParametricShape::CircularHollow {
                    diameter: 168.3 * MM,
                    thickness: 8.0 * MM,
                },
                ParametricShape::LShape {
                    height: 100.0 * MM,
                    width: 100.0 * MM,
                    thickness: 10.0 * MM,
                    root_radius: 12.0 * MM,
                    toe_radius: 6.0 * MM,
                },
            ];
            for shape in typical.into_iter().chain(ParametricShape::ALL) {
                let (project, profile) = parametric(shape);
                assert_eq!(validate_profile(&project, &profile), Ok(profile), "{}", shape.code());
            }
        }

        /// PRF-005: a dimension that is zero, negative, NaN or above
        /// MAX_PROFILE_DIMENSION is rejected with a message naming it; a
        /// radius may be zero.
        #[test]
        fn out_of_range_dimensions_are_rejected_naming_the_dimension() {
            for shape in ParametricShape::ALL {
                for (dimension, _) in shape.dimensions() {
                    let mut bad = vec![-MM, f64::NAN, MAX_PROFILE_DIMENSION * 1.01, f64::INFINITY];
                    if dimension.is_radius() {
                        assert!(
                            check(with(shape, dimension, 0.0)).is_ok(),
                            "{} {:?} 0",
                            shape.code(),
                            dimension
                        );
                    } else {
                        bad.push(0.0);
                    }
                    for value in bad {
                        let error = check(with(shape, dimension, value)).unwrap_err();
                        assert_eq!(
                            error.key(),
                            "model-error-profile-dimension-range",
                            "{} {dimension:?} {value}",
                            shape.code()
                        );
                        assert_eq!(named_dimension(&error), dimension.label_key());
                    }
                }
            }
        }

        /// The key and the named dimension of the error for `shape`.
        fn limit_error(shape: ParametricShape) -> (&'static str, &'static str) {
            let error = check(shape).unwrap_err();
            (error.key(), named_dimension(&error))
        }

        /// PRF-005: dimensions that do not fit together are rejected naming
        /// the dimension: two flanges as deep as the height, a hollow
        /// section's wall of half its smaller side, a web as wide as the
        /// flange, and a root radius that does not fit between the web and
        /// the flange tip.
        #[test]
        fn inconsistent_dimensions_are_rejected_naming_the_dimension() {
            let limit = "model-error-profile-dimension-limit";
            let radius = "model-error-profile-radius-limit";
            // 2 × flange >= height, also for the channel and the Z.
            for shape in [standard("i"), standard("u"), standard("z")] {
                let half_height = shape.size().y / 2.0;
                assert_eq!(
                    limit_error(with(shape, D::Flange, half_height)),
                    (limit, D::Flange.label_key()),
                    "{}",
                    shape.code()
                );
                let width = shape.size().x;
                assert_eq!(limit_error(with(shape, D::Web, width)), (limit, D::Web.label_key()));
            }
            // A T's flange as deep as the whole height.
            let t = standard("t");
            assert_eq!(
                limit_error(with(t, D::Flange, t.size().y)),
                (limit, D::Flange.label_key())
            );
            // RHS 200 × 100: the wall must be thinner than 50 mm.
            let rhs = standard("rectangular_hollow");
            assert!(check(with(with(rhs, D::OuterRadius, 0.0), D::Thickness, 49.0 * MM)).is_ok());
            assert_eq!(
                limit_error(with(with(rhs, D::OuterRadius, 0.0), D::Thickness, 50.0 * MM)),
                (limit, D::Thickness.label_key())
            );
            assert_eq!(
                limit_error(with(rhs, D::OuterRadius, 51.0 * MM)),
                (radius, D::OuterRadius.label_key())
            );
            // CHS 168.3: the wall must be thinner than half the diameter.
            let chs = standard("circular_hollow");
            assert_eq!(
                limit_error(with(chs, D::Thickness, 168.3 * MM / 2.0)),
                (limit, D::Thickness.label_key())
            );
            // IPE 200: the root radius fits in (100 - 5.6) / 2 = 47.2 mm.
            assert!(check(with(ipe_200(), D::RootRadius, 47.2 * MM)).is_ok());
            let error = check(with(ipe_200(), D::RootRadius, 47.3 * MM)).unwrap_err();
            assert_eq!(error.key(), radius);
            assert_eq!(named_dimension(&error), D::RootRadius.label_key());
            assert_eq!(error.arg("max"), Some(&Arg::Length((100.0 * MM - 5.6 * MM) / 2.0)));
            // The angle's legs: thickness below the shorter leg, the root
            // radius in the leg beyond the other leg, the toe radius at most
            // the thickness.
            let l = standard("l");
            assert_eq!(
                limit_error(with(l, D::Thickness, 100.0 * MM)),
                (limit, D::Thickness.label_key())
            );
            assert_eq!(
                limit_error(with(l, D::RootRadius, 91.0 * MM)),
                (radius, D::RootRadius.label_key())
            );
            assert_eq!(
                limit_error(with(l, D::ToeRadius, 11.0 * MM)),
                (radius, D::ToeRadius.label_key())
            );
            // L 100 × 100 × 10: r1 + r2 must fit on the 90 mm inner face, so
            // r1 85 mm leaves room for an r2 of 5 mm, not 10 mm.
            let tight = with(l, D::RootRadius, 85.0 * MM);
            assert!(check(with(tight, D::ToeRadius, 5.0 * MM)).is_ok());
            let error = check(with(tight, D::ToeRadius, 10.0 * MM)).unwrap_err();
            assert_eq!(error.key(), radius);
            assert_eq!(named_dimension(&error), D::ToeRadius.label_key());
        }

        /// PRF-004: a profile needs a name, its materials must exist and
        /// its origin must lie within MAX_PROFILE_DIMENSION of zero, as a
        /// distance: (8 m, 8 m) lies farther, 11.3 m.
        #[test]
        fn names_materials_and_origins_are_checked() {
            let (project, profile) = parametric(ipe_200());
            let blank = Profile {
                name: "  ".into(),
                ..profile.clone()
            };
            assert_eq!(
                validate_profile(&project, &blank).unwrap_err().key(),
                "model-error-profile-name"
            );
            let missing = Profile {
                shape: ProfileShape::Parametric {
                    shape: ipe_200(),
                    material: MaterialId::new(),
                },
                ..profile.clone()
            };
            assert_eq!(
                validate_profile(&project, &missing).unwrap_err().key(),
                "model-error-profile-material-missing"
            );
            let offset = Profile {
                origin: DVec2::new(0.05, -0.1),
                ..profile.clone()
            };
            assert!(validate_profile(&project, &offset).is_ok());
            let edge = Profile {
                origin: DVec2::new(0.0, -MAX_PROFILE_DIMENSION),
                ..profile.clone()
            };
            assert!(validate_profile(&project, &edge).is_ok());
            for origin in [
                DVec2::new(MAX_PROFILE_DIMENSION * 1.01, 0.0),
                DVec2::splat(0.8 * MAX_PROFILE_DIMENSION),
                DVec2::new(0.0, f64::NAN),
            ] {
                let far = Profile {
                    origin,
                    ..profile.clone()
                };
                assert_eq!(
                    validate_profile(&project, &far).unwrap_err().key(),
                    "model-error-profile-origin"
                );
            }
        }

        fn rect(min: DVec2, size: DVec2) -> Vec<DVec2> {
            vec![
                min,
                min + DVec2::new(size.x, 0.0),
                min + size,
                min + DVec2::new(0.0, size.y),
            ]
        }

        /// The outer ring and the holes of a custom region.
        type Region = (Vec<DVec2>, Vec<Vec<DVec2>>);

        /// The project and a custom profile of `regions` (outer ring and
        /// holes), each of the project's first material.
        fn custom(regions: Vec<Region>) -> (Project, Profile) {
            let project = default_metric_project();
            let material = project.materials[0].id;
            let regions = regions
                .into_iter()
                .map(|(outer, holes)| ProfileRegion { outer, holes, material })
                .collect();
            (project, Profile::custom("Custom", regions))
        }

        fn check_custom(regions: Vec<Region>) -> Result<Profile, Message> {
            let (project, profile) = custom(regions);
            validate_profile(&project, &profile)
        }

        /// The nested reason of a region error, after checking its number.
        fn region_reason(error: &Message, number: usize) -> &'static str {
            assert_eq!(error.key(), "model-error-profile-region");
            assert_eq!(error.arg("number"), Some(&Arg::Count(number as u64)));
            match error.arg("reason") {
                Some(Arg::Nested(reason)) => reason.key(),
                other => panic!("no reason: {other:?}"),
            }
        }

        /// PRF-004: a custom profile without regions, with a
        /// self-intersecting ring, with a hole outside its region or with
        /// two overlapping regions is rejected.
        #[test]
        fn invalid_custom_profiles_are_rejected() {
            assert_eq!(
                check_custom(Vec::new()).unwrap_err().key(),
                "model-error-profile-no-regions"
            );
            let block = rect(DVec2::ZERO, DVec2::splat(0.2));
            // A figure eight with lobes of different sizes.
            let bow_tie = vec![
                DVec2::new(0.0, 0.0),
                DVec2::new(0.2, 0.2),
                DVec2::new(0.2, 0.0),
                DVec2::new(0.0, 0.1),
            ];
            let error = check_custom(vec![(block.clone(), Vec::new()), (bow_tie, Vec::new())]).unwrap_err();
            assert_eq!(region_reason(&error, 2), "model-error-polygon-self-intersection");
            let outside = rect(DVec2::splat(0.3), DVec2::splat(0.05));
            let error = check_custom(vec![(block.clone(), vec![outside])]).unwrap_err();
            assert_eq!(region_reason(&error, 1), "model-error-hole-outside");
            let overlapping = rect(DVec2::splat(0.1), DVec2::splat(0.2));
            let error = check_custom(vec![(block.clone(), Vec::new()), (overlapping, Vec::new())]).unwrap_err();
            assert_eq!(error.key(), "model-error-profile-regions-overlap");
            assert_eq!(error.arg("first"), Some(&Arg::Count(1)));
            assert_eq!(error.arg("second"), Some(&Arg::Count(2)));
            // The same region twice, and one inside another, overlap too.
            let error = check_custom(vec![(block.clone(), Vec::new()), (block.clone(), Vec::new())]).unwrap_err();
            assert_eq!(error.key(), "model-error-profile-regions-overlap");
            let inner = rect(DVec2::splat(0.05), DVec2::splat(0.1));
            let error = check_custom(vec![(block, Vec::new()), (inner, Vec::new())]).unwrap_err();
            assert_eq!(error.key(), "model-error-profile-regions-overlap");
        }

        /// PRF-004: regions may touch: two sharing an edge, one sharing
        /// part of an edge or a corner, and a core filling another's hole
        /// are accepted.
        #[test]
        fn touching_regions_are_accepted() {
            let block = rect(DVec2::ZERO, DVec2::splat(0.2));
            let beside = rect(DVec2::new(0.2, 0.0), DVec2::splat(0.2));
            let partly = rect(DVec2::new(0.2, 0.1), DVec2::new(0.1, 0.3));
            let corner = rect(DVec2::splat(0.2), DVec2::splat(0.1));
            let regions = vec![(block.clone(), Vec::new()), (beside, Vec::new()), (corner, Vec::new())];
            assert!(check_custom(regions).is_ok());
            assert!(check_custom(vec![(block.clone(), Vec::new()), (partly, Vec::new())]).is_ok());
            // A tube with a core of another material in its hole.
            let hole = rect(DVec2::splat(0.05), DVec2::splat(0.1));
            assert!(check_custom(vec![(block, vec![hole.clone()]), (hole, Vec::new())]).is_ok());
        }

        /// PRF-004: valid custom regions come back with their outer rings
        /// counter-clockwise and their holes clockwise, whatever the input.
        #[test]
        fn custom_rings_are_normalised() {
            let mut outer = rect(DVec2::ZERO, DVec2::splat(0.2));
            outer.reverse();
            let hole = rect(DVec2::splat(0.05), DVec2::splat(0.1));
            let second = rect(DVec2::new(0.2, 0.0), DVec2::splat(0.2));
            let profile = check_custom(vec![(outer, vec![hole]), (second, Vec::new())]).unwrap();
            let ProfileShape::Custom { regions } = &profile.shape else {
                panic!("custom");
            };
            for region in regions {
                assert!(signed_area(&region.outer) > 0.0);
                for hole in &region.holes {
                    assert!(signed_area(hole) < 0.0);
                }
            }
            assert_eq!(regions[0].holes.len(), 1);
        }

        /// PRF-004: a custom profile has at most MAX_SLAB_POINTS points in
        /// all its regions.
        #[test]
        fn custom_profiles_are_limited_in_points() {
            let comb = comb((MAX_SLAB_POINTS - 3) / 4);
            let far = moved(&square(1.0), DVec2::new(0.0, -10.0));
            let error = check_custom(vec![(comb, Vec::new()), (far, Vec::new())]).unwrap_err();
            assert_eq!(
                error,
                Message::new("model-error-profile-points").count("max", MAX_SLAB_POINTS)
            );
        }

        /// PRF-004: a custom profile has at most MAX_PROFILE_REGIONS
        /// regions.
        #[test]
        fn custom_profiles_are_limited_in_regions() {
            let squares = |n: usize| {
                (0..n)
                    .map(|i| (rect(DVec2::new(0.2 * i as f64, 0.0), DVec2::splat(0.1)), Vec::new()))
                    .collect::<Vec<_>>()
            };
            assert!(check_custom(squares(MAX_PROFILE_REGIONS)).is_ok());
            assert_eq!(
                check_custom(squares(MAX_PROFILE_REGIONS + 1)).unwrap_err(),
                Message::new("model-error-profile-regions").count("max", MAX_PROFILE_REGIONS)
            );
        }

        /// PRF-004: custom regions together are at most
        /// MAX_PROFILE_DIMENSION wide and tall, wherever they lie: one
        /// region 10 m wide is accepted, one wider, one taller and two small
        /// ones farther apart are rejected naming the limit.
        #[test]
        fn custom_profiles_are_limited_in_size() {
            let far = DVec2::splat(1000.0);
            let wide = rect(far, DVec2::new(MAX_PROFILE_DIMENSION, 0.1));
            assert!(check_custom(vec![(wide, Vec::new())]).is_ok());
            let too_large = Message::new("model-error-profile-size").length("max", MAX_PROFILE_DIMENSION);
            for size in [DVec2::new(10.01, 0.1), DVec2::new(0.1, 10.01)] {
                let error = check_custom(vec![(rect(far, size), Vec::new())]).unwrap_err();
                assert_eq!(error, too_large, "{size}");
            }
            let small = DVec2::splat(0.1);
            let apart = vec![
                (rect(DVec2::ZERO, small), Vec::new()),
                (rect(DVec2::new(0.0, 9.95), small), Vec::new()),
            ];
            assert_eq!(check_custom(apart).unwrap_err(), too_large);
        }

        /// `n` points on the arc of `radius` about zero from angle `from`
        /// to `to`, both included.
        fn arc(radius: f64, from: f64, to: f64, n: usize) -> Vec<DVec2> {
            (0..n)
                .map(|i| DVec2::from_angle(from + (to - from) * i as f64 / (n - 1) as f64) * radius)
                .collect()
        }

        /// A full circle of `n` points.
        fn circle(radius: f64, n: usize) -> Vec<DVec2> {
            let mut ring = arc(radius, 0.0, TAU, n + 1);
            ring.pop();
            ring
        }

        /// A comb of `teeth` teeth `width` wide every `pitch`, `length`
        /// long, on a base 5 mm deep: `4 * teeth + 3` points.
        fn fine_comb(teeth: usize, pitch: f64, width: f64, length: f64) -> Vec<DVec2> {
            let mut ring = Vec::new();
            for i in 0..teeth {
                let x = pitch * i as f64;
                ring.extend([
                    DVec2::new(x, 0.0),
                    DVec2::new(x, length),
                    DVec2::new(x + width, length),
                    DVec2::new(x + width, 0.0),
                ]);
            }
            let end = pitch * teeth as f64;
            ring.extend([DVec2::new(end, 0.0), DVec2::new(end, -0.005), DVec2::new(0.0, -0.005)]);
            ring
        }

        /// `n` points from x 0 to `length`, alternately at heights 0.5 -
        /// `amplitude` and 0.5 + `amplitude`, the first one low.
        fn zigzag(n: usize, length: f64, amplitude: f64) -> Vec<DVec2> {
            (0..n)
                .map(|i| {
                    let up = if i % 2 == 0 { -amplitude } else { amplitude };
                    DVec2::new(length * i as f64 / (n - 1) as f64, 0.5 + up)
                })
                .collect()
        }

        /// The region below or above a zigzag from x 0 to 9.9 m.
        fn under(zigzag: &[DVec2]) -> Vec<DVec2> {
            let mut ring = vec![DVec2::new(0.0, 0.0), DVec2::new(9.9, 0.0)];
            ring.extend(zigzag.iter().rev());
            ring
        }

        fn over(zigzag: &[DVec2]) -> Vec<DVec2> {
            let mut ring = zigzag.to_vec();
            ring.extend([DVec2::new(9.9, 1.0), DVec2::new(0.0, 1.0)]);
            ring
        }

        /// Custom profiles of about MAX_SLAB_POINTS points at the worst for
        /// the overlap check, with the error each gives, if any.
        ///
        /// MAX_PROFILE_REGIONS regions whose boxes all overlap, so every
        /// pair is checked: a fan of wedges about one point, each with an
        /// arc of points; a tube of thousands of points around a fan of
        /// triangles in its hole, each checked against the tube; and the
        /// same with the last triangle overlapping its neighbour, which is
        /// rejected at the last pair.
        ///
        /// Two regions of 5,000 points whose outlines cross millions of
        /// times: a comb of 1,249 teeth and the same comb turned by 90°,
        /// and two zigzags of opposite phase. Their intersections have
        /// millions of points; the sweep for crossings finds a deep one
        /// instead. Two regions on either side of one zigzag of 5,000
        /// points only touch.
        fn profiles_at_the_limit() -> Vec<(&'static str, Vec<Region>, Option<&'static str>)> {
            let overlap = Some("model-error-profile-regions-overlap");
            let n = MAX_PROFILE_REGIONS;
            let step = TAU / n as f64;
            let fan = (0..n)
                .map(|k| {
                    let from = step * k as f64;
                    let mut wedge = vec![DVec2::ZERO];
                    wedge.extend(arc(4.0, from, from + 0.9 * step, MAX_SLAB_POINTS / n - 1));
                    (wedge, Vec::new())
                })
                .collect();
            let triangles = n - 1;
            let tube_points = (MAX_SLAB_POINTS - 3 * triangles) / 2;
            let step = TAU / triangles as f64;
            let triangle = |from: f64, angle: f64| {
                vec![
                    DVec2::ZERO,
                    DVec2::from_angle(from) * 2.0,
                    DVec2::from_angle(from + angle) * 2.0,
                ]
            };
            let tube = (circle(3.5, tube_points), vec![circle(3.0, tube_points)]);
            let tube_fan: Vec<_> = std::iter::once(tube)
                .chain((0..triangles).map(|k| (triangle(step * k as f64, 0.5 * step), Vec::new())))
                .collect();
            // From the middle of the previous triangle's gap back into it.
            let mut overlapping = tube_fan.clone();
            overlapping[n - 1].0 = triangle(step * (triangles as f64 - 1.75), 1.25 * step);
            let comb = fine_comb(1249, 0.0079, 0.004, 9.9);
            let turned: Vec<DVec2> = comb.iter().map(|p| DVec2::new(p.y, p.x)).collect();
            let low = zigzag(4998, 9.9, 0.2);
            let high: Vec<DVec2> = low.iter().map(|p| DVec2::new(p.x, 1.0 - p.y)).collect();
            vec![
                ("fan", fan, None),
                ("tube around a fan", tube_fan, None),
                ("tube around an overlapping fan", overlapping, overlap),
                ("crossed combs", vec![(comb, Vec::new()), (turned, Vec::new())], overlap),
                (
                    "crossing zigzags",
                    vec![(under(&low), Vec::new()), (over(&high), Vec::new())],
                    overlap,
                ),
                (
                    "regions sharing a zigzag",
                    vec![(under(&low), Vec::new()), (over(&low), Vec::new())],
                    None,
                ),
            ]
        }

        /// NFR-SEC-003, PRF-004: the region checks grow with the regions
        /// times the points, not with the crossings of their outlines, so
        /// each profile of [`profiles_at_the_limit`] validates, or is
        /// rejected, within the time budget of
        /// `slabs_at_the_point_limit_validate_quickly`: 100 ms in a release
        /// build, 250 ms in test builds. A fan of 2,500
        /// wedges, whose 3 million pairs took about 2 s to intersect, is
        /// rejected for its number of regions before any is checked.
        #[test]
        fn custom_profiles_at_the_limit_validate_quickly() {
            let budget = Duration::from_millis(if cfg!(debug_assertions) { 250 } else { 100 });
            let wedges = 2500;
            let step = TAU / wedges as f64;
            let slivers = (0..wedges)
                .map(|k| (arc(1.0, step * k as f64, step * (k as f64 + 0.9), 2), Vec::new()))
                .map(|(mut wedge, holes): (Vec<DVec2>, _)| {
                    wedge.insert(0, DVec2::ZERO);
                    (wedge, holes)
                })
                .collect();
            let (project, profile) = custom(slivers);
            let start = Instant::now();
            let result = validate_profile(&project, &profile);
            let elapsed = start.elapsed();
            assert_eq!(result.unwrap_err().key(), "model-error-profile-regions");
            assert!(elapsed < budget, "slivers took {elapsed:?}");
            for (name, regions, expected) in profiles_at_the_limit() {
                let count = regions.len();
                let points: usize = regions
                    .iter()
                    .map(|(outer, holes)| outer.len() + holes.iter().map(Vec::len).sum::<usize>())
                    .sum();
                assert!(
                    (MAX_SLAB_POINTS - 40..=MAX_SLAB_POINTS).contains(&points),
                    "{name}: {points} points"
                );
                let (project, profile) = custom(regions);
                let start = Instant::now();
                let result = validate_profile(&project, &profile);
                let elapsed = start.elapsed();
                match expected {
                    Some(key) => {
                        let error = result.unwrap_err();
                        assert_eq!(error.key(), key, "{name}");
                        assert_eq!(error.arg("first"), Some(&Arg::Count(count as u64 - 1)), "{name}");
                        assert_eq!(error.arg("second"), Some(&Arg::Count(count as u64)), "{name}");
                    }
                    None => assert!(result.is_ok(), "{name}: {result:?}"),
                }
                assert!(elapsed < budget, "{name} took {elapsed:?}");
            }
        }
    }
}
