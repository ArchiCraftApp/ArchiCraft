//! Commands of the View menu (VIEW-014, UI-006).

use egui::Key;

use super::{Binding, Category, CommandSpec, Scope};

pub(super) fn specs() -> Vec<CommandSpec> {
    let view = |id| CommandSpec::new(id, Category::View);
    vec![
        view("view.fit-window")
            .menu("view", 0, 0)
            .bind(Binding::key(Key::F))
            .scope(Scope::Canvas),
        view("view.zoom-selection")
            .menu("view", 0, 1)
            .bind(Binding::key(Key::F).shift()),
        view("view.previous-zoom")
            .menu("view", 0, 2)
            .bind(Binding::primary(Key::OpenBracket)),
        view("view.next-zoom")
            .menu("view", 0, 3)
            .bind(Binding::primary(Key::CloseBracket)),
        view("view.switch-plan-3d")
            .menu("view", 1, 0)
            .bind(Binding::key(Key::F3))
            .scope(Scope::Canvas),
        // Ctrl+Tab stays off while a tool takes point input, where Tab moves
        // between the typed fields.
        view("view.next-tab")
            .menu("view", 1, 1)
            .bind(Binding::ctrl(Key::Tab))
            .scope(Scope::Canvas),
        view("view.previous-tab")
            .menu("view", 1, 2)
            .bind(Binding::ctrl(Key::Tab).shift()),
        view("view.orbit")
            .menu("view", 1, 3)
            .bind(Binding::key(Key::O))
            .scope(Scope::Canvas),
        view("view.reference-lines").menu("view", 2, 0),
        view("view.problems")
            .menu("view", 2, 1)
            .bind(Binding::primary(Key::P).shift())
            .scope(Scope::TextSafe),
    ]
}
