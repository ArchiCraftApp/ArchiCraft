//! Sample projects, built through commands like a user would build them. They
//! serve as reference models for golden-file and conformance tests (SRS 7.3)
//! and as a starting point in the application.

use glam::DVec2;

use crate::command::{Command, History};
use crate::element::{ElementKind, Position};
use crate::kinds::ReferenceLine;
use crate::project::Project;
use crate::template::default_metric_project;

fn rectangle(min: DVec2, max: DVec2) -> Vec<DVec2> {
    vec![min, DVec2::new(max.x, min.y), max, DVec2::new(min.x, max.y)]
}

/// Adds a closed ring of joined exterior walls, counter-clockwise.
fn add_wall_ring(project: &mut Project, history: &mut History, storey: crate::StoreyId, corners: &[DVec2]) {
    let walls = (0..corners.len())
        .map(|i| project.new_wall(storey, corners[i], corners[(i + 1) % corners.len()], i as u32))
        .collect();
    execute(project, history, Command::AddElements(walls));
}

fn execute(project: &mut Project, history: &mut History, command: Command) {
    if let Err(e) = history.execute(project, command) {
        unreachable!("sample commands are valid: {e}");
    }
}

/// The exit criterion of SRS increment 0: one storey with four exterior
/// walls joined at the corners and a floor slab.
pub fn storey_with_walls_and_slab() -> Project {
    let mut project = default_metric_project();
    let mut history = History::default();
    project.info.name = "Walls and slab".into();
    let ground = project.storeys.ground();
    let outline = rectangle(DVec2::ZERO, DVec2::new(10.0, 8.0));
    add_wall_ring(&mut project, &mut history, ground, &outline);
    let slab = project.new_slab(ground, outline, Vec::new(), 0);
    execute(&mut project, &mut history, Command::AddElements(vec![slab]));
    project
}

/// A two-storey shell: exterior walls on both storeys, an interior wall in a
/// T junction, a ground slab and an upper floor slab with a stair opening.
pub fn two_storey_shell() -> Project {
    let mut project = default_metric_project();
    let mut history = History::default();
    project.info.name = "Two-storey shell".into();
    let ground = project.storeys.ground();
    let upper = project.storeys.as_slice()[1].id;
    let outline = rectangle(DVec2::ZERO, DVec2::new(12.0, 9.0));

    for storey in [ground, upper] {
        add_wall_ring(&mut project, &mut history, storey, &outline);
        // Interior wall from the south wall's reference line northwards.
        let mut interior = project.new_wall(storey, DVec2::new(5.0, 0.0), DVec2::new(5.0, 6.0), 0);
        interior.position = Position::Interior;
        interior.layer = project
            .layers
            .iter()
            .find(|l| l.name == "Interior walls")
            .map_or(interior.layer, |l| l.id);
        if let ElementKind::Wall(wall) = &mut interior.kind {
            wall.thickness = 0.115;
            wall.reference_line = ReferenceLine::Center;
            if let Some(m) = project.materials.iter().find(|m| m.name == "Sand-lime brick") {
                wall.material = m.id;
            }
        }
        execute(&mut project, &mut history, Command::AddElements(vec![interior]));
    }

    let ground_slab = project.new_slab(ground, outline.clone(), Vec::new(), 0);
    let stair_hole = rectangle(DVec2::new(6.0, 1.0), DVec2::new(9.0, 2.2));
    let upper_slab = project.new_slab(upper, outline, vec![stair_hole], 1);
    execute(
        &mut project,
        &mut history,
        Command::AddElements(vec![ground_slab, upper_slab]),
    );
    project
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_are_valid() {
        let p = storey_with_walls_and_slab();
        assert_eq!(p.elements.len(), 5);
        let p = two_storey_shell();
        assert_eq!(p.elements.len(), 12);
        assert!(p.integrity_problems().is_empty());
    }
}
