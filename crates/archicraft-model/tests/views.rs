//! Saved camera views: a 3D view stores a camera, and saving, updating,
//! renaming and deleting a view are undoable commands (VIEW-004, VIEW-003,
//! UND-001).

#![allow(clippy::unwrap_used)]

use archicraft_model::glam::DVec3;
use archicraft_model::project::{Projection, View3d, ViewCamera};
use archicraft_model::template::default_metric_project;
use archicraft_model::{Command, CommandError, History, Project, ViewId};

fn camera() -> ViewCamera {
    ViewCamera {
        target: DVec3::new(2.0, 3.0, 1.25),
        distance: 18.5,
        yaw: 0.7,
        pitch: 0.35,
        fov_y: 50.0_f64.to_radians(),
    }
}

fn other_camera() -> ViewCamera {
    ViewCamera {
        target: DVec3::new(-1.0, 4.5, 0.0),
        distance: 7.0,
        yaw: -2.1,
        pitch: -0.2,
        fov_y: 30.0_f64.to_radians(),
    }
}

fn save(view: ViewId, projection: Projection, camera: ViewCamera) -> Command {
    Command::SaveCamera {
        view,
        base: "Camera".into(),
        projection,
        camera,
    }
}

/// VIEW-004: Save camera adds a 3D view with the camera and the projection
/// it was given, named "Camera 1"; undo removes it and redo brings back the
/// same view.
#[test]
fn save_camera_adds_a_named_view_with_the_camera_undoably() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let before = p.views.clone();
    let id = ViewId::new();
    h.execute(&mut p, save(id, Projection::Perspective, camera())).unwrap();

    let view = p.view(id).unwrap();
    assert_eq!(view.name, "Camera 1");
    assert_eq!(view.projection, Projection::Perspective);
    assert_eq!(view.camera, Some(camera()));
    assert_eq!(p.views.len(), before.len() + 1);
    assert_eq!(p.views.last().unwrap().id, id, "a new view goes after the others");

    let label = h.undo(&mut p).unwrap();
    assert_eq!(label.key(), "model-label-save-camera");
    assert_eq!(p.views, before);
    h.redo(&mut p).unwrap();
    assert_eq!(p.view(id).unwrap().camera, Some(camera()));
    assert_eq!(p.view(id).unwrap().name, "Camera 1");
}

/// VIEW-004: each save takes the first number no view name uses yet, so a
/// gap left by a renamed or deleted camera is filled and a name the user
/// gave is never repeated.
#[test]
fn save_camera_takes_the_first_free_number() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let first = ViewId::new();
    let second = ViewId::new();
    h.execute(&mut p, save(first, Projection::Perspective, camera()))
        .unwrap();
    h.execute(&mut p, save(second, Projection::Parallel, camera())).unwrap();
    assert_eq!(p.view(second).unwrap().name, "Camera 2");
    assert_eq!(p.view(second).unwrap().projection, Projection::Parallel);

    h.execute(
        &mut p,
        Command::RenameView {
            view: first,
            name: "Garden".into(),
        },
    )
    .unwrap();
    let third = ViewId::new();
    h.execute(&mut p, save(third, Projection::Perspective, camera()))
        .unwrap();
    assert_eq!(p.view(third).unwrap().name, "Camera 1", "the gap is filled");

    h.execute(
        &mut p,
        Command::RenameView {
            view: third,
            name: "Camera 3".into(),
        },
    )
    .unwrap();
    let fourth = ViewId::new();
    h.execute(&mut p, save(fourth, Projection::Perspective, camera()))
        .unwrap();
    assert_eq!(p.view(fourth).unwrap().name, "Camera 1");
    let fifth = ViewId::new();
    h.execute(&mut p, save(fifth, Projection::Perspective, camera()))
        .unwrap();
    assert_eq!(p.view(fifth).unwrap().name, "Camera 4", "2 and 3 are taken");
}

/// VIEW-004: Update camera redefines a view from the current window,
/// keeping its identity, name and position in the list; undo restores the
/// camera before, redo the new one. A template view without a camera gets
/// one the same way.
#[test]
fn update_camera_replaces_the_camera_undoably() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let id = ViewId::new();
    h.execute(&mut p, save(id, Projection::Perspective, camera())).unwrap();
    let index = p.views.iter().position(|v| v.id == id).unwrap();

    h.execute(
        &mut p,
        Command::SetViewCamera {
            view: id,
            camera: other_camera(),
        },
    )
    .unwrap();
    assert_eq!(p.view(id).unwrap().camera, Some(other_camera()));
    assert_eq!(p.view(id).unwrap().name, "Camera 1");
    assert_eq!(p.views[index].id, id);

    let label = h.undo(&mut p).unwrap();
    assert_eq!(label.key(), "model-label-update-camera");
    assert_eq!(p.view(id).unwrap().camera, Some(camera()));
    h.redo(&mut p).unwrap();
    assert_eq!(p.view(id).unwrap().camera, Some(other_camera()));

    let template = p.views[0].id;
    assert_eq!(p.views[0].camera, None);
    h.execute(
        &mut p,
        Command::SetViewCamera {
            view: template,
            camera: camera(),
        },
    )
    .unwrap();
    assert_eq!(p.view(template).unwrap().camera, Some(camera()));
    h.undo(&mut p).unwrap();
    assert_eq!(p.view(template).unwrap().camera, None);
}

/// VIEW-003: renaming a view trims the name and is one undo step; an empty
/// name is refused and the same name changes nothing.
#[test]
fn rename_view_is_undoable() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let id = p.views[0].id;
    let old = p.views[0].name.clone();
    h.execute(
        &mut p,
        Command::RenameView {
            view: id,
            name: "  Street side ".into(),
        },
    )
    .unwrap();
    assert_eq!(p.view(id).unwrap().name, "Street side");
    let label = h.undo(&mut p).unwrap();
    assert_eq!(label.key(), "model-label-rename-view");
    assert_eq!(p.view(id).unwrap().name, old);

    let before = p.clone();
    let depth = h.undo_len();
    let empty = h.execute(
        &mut p,
        Command::RenameView {
            view: id,
            name: "   ".into(),
        },
    );
    assert_eq!(empty.unwrap_err().message().key(), "model-error-view-name");
    let same = h.execute(&mut p, Command::RenameView { view: id, name: old });
    assert_eq!(same.unwrap_err(), CommandError::NoChange);
    assert_eq!(p, before);
    assert_eq!(h.undo_len(), depth);
}

/// VIEW-003: deleting a view removes it; undo puts it back at its place in
/// the list with its camera, redo removes it again.
#[test]
fn delete_view_is_undoable_and_restores_its_place() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let first = ViewId::new();
    let second = ViewId::new();
    h.execute(&mut p, save(first, Projection::Perspective, camera()))
        .unwrap();
    h.execute(&mut p, save(second, Projection::Parallel, other_camera()))
        .unwrap();
    let before = p.views.clone();

    h.execute(&mut p, Command::DeleteView { view: first }).unwrap();
    assert!(p.view(first).is_none());
    assert_eq!(p.views.len(), before.len() - 1);

    let label = h.undo(&mut p).unwrap();
    assert_eq!(label.key(), "model-label-delete-view");
    assert_eq!(p.views, before);
    h.redo(&mut p).unwrap();
    assert!(p.view(first).is_none());
    assert!(p.view(second).is_some());
}

/// VIEW-004, NFR-REL-005: a camera with a non-finite value, a distance that
/// is not positive, a pitch beyond the poles or a field of view outside
/// (0, pi) is refused by Save camera and Update camera, and the project and
/// the history stay as they were.
#[test]
fn invalid_cameras_are_rejected() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let id = ViewId::new();
    h.execute(&mut p, save(id, Projection::Perspective, camera())).unwrap();
    let before = p.clone();
    let depth = h.undo_len();

    let bad = [
        ViewCamera {
            target: DVec3::new(f64::NAN, 0.0, 0.0),
            ..camera()
        },
        ViewCamera {
            target: DVec3::new(0.0, f64::INFINITY, 0.0),
            ..camera()
        },
        ViewCamera {
            distance: 0.0,
            ..camera()
        },
        ViewCamera {
            distance: -3.0,
            ..camera()
        },
        ViewCamera {
            distance: f64::INFINITY,
            ..camera()
        },
        ViewCamera {
            yaw: f64::NAN,
            ..camera()
        },
        ViewCamera { pitch: 2.0, ..camera() },
        ViewCamera { fov_y: 0.0, ..camera() },
        ViewCamera {
            fov_y: std::f64::consts::PI,
            ..camera()
        },
    ];
    for camera in bad {
        assert!(camera.validate().is_err(), "{camera:?}");
        let saved = h.execute(&mut p, save(ViewId::new(), Projection::Perspective, camera));
        assert_eq!(
            saved.unwrap_err().message().key(),
            "model-error-view-camera",
            "{camera:?}"
        );
        let updated = h.execute(&mut p, Command::SetViewCamera { view: id, camera });
        assert_eq!(updated.unwrap_err().message().key(), "model-error-view-camera");
    }
    assert!(camera().validate().is_ok());
    assert_eq!(p, before);
    assert_eq!(h.undo_len(), depth);
}

/// A command naming a view that does not exist is refused, as is saving
/// under an identity a view already has or with an empty base name, and
/// nothing changes.
#[test]
fn unknown_views_and_duplicate_ids_are_rejected() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let before = p.clone();
    let missing = ViewId::new();
    let not_found = |r: Result<(), CommandError>| {
        let e = r.unwrap_err();
        assert!(matches!(e, CommandError::NotFound(_)), "{e:?}");
        assert_eq!(e.message().key(), "model-error-view-not-found");
    };
    not_found(h.execute(
        &mut p,
        Command::SetViewCamera {
            view: missing,
            camera: camera(),
        },
    ));
    not_found(h.execute(
        &mut p,
        Command::RenameView {
            view: missing,
            name: "X".into(),
        },
    ));
    not_found(h.execute(&mut p, Command::DeleteView { view: missing }));

    let taken = p.views[0].id;
    let duplicate = h.execute(&mut p, save(taken, Projection::Perspective, camera()));
    assert_eq!(duplicate.unwrap_err().message().key(), "model-error-duplicate-view-id");
    let unnamed = h.execute(
        &mut p,
        Command::SaveCamera {
            view: ViewId::new(),
            base: "  ".into(),
            projection: Projection::Perspective,
            camera: camera(),
        },
    );
    assert_eq!(unnamed.unwrap_err().message().key(), "model-error-view-name");
    assert_eq!(p, before);
    assert_eq!(h.undo_len(), 0);
}

/// Setting the camera a view already has changes nothing.
#[test]
fn updating_to_the_same_camera_changes_nothing() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let id = ViewId::new();
    h.execute(&mut p, save(id, Projection::Perspective, camera())).unwrap();
    let same = h.execute(
        &mut p,
        Command::SetViewCamera {
            view: id,
            camera: camera(),
        },
    );
    assert_eq!(same.unwrap_err(), CommandError::NoChange);
    assert_eq!(h.undo_len(), 1);
}

/// A view saved before cameras existed (the template's, or one in an older
/// journal or recovery file) deserializes without a camera.
#[test]
fn views_without_a_camera_still_deserialize() {
    let json = format!(
        r#"{{"id":"{}","name":"3D perspective","projection":"Perspective"}}"#,
        ViewId::new()
    );
    let view: View3d = serde_json::from_str(&json).unwrap();
    assert_eq!(view.camera, None);
    let p: &Project = &default_metric_project();
    assert!(p.views.iter().all(|v| v.camera.is_none()));
}
