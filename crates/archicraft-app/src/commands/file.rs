//! Commands of the File menu (UI-011, PRJ-003, PRJ-004).

use egui::Key;

use super::{Binding, Category, CommandSpec, Scope};

pub(super) fn specs() -> Vec<CommandSpec> {
    let file = |id| CommandSpec::new(id, Category::File).scope(Scope::TextSafe);
    vec![
        file("file.new").menu("file", 0, 0).bind(Binding::primary(Key::N)),
        file("file.open").menu("file", 0, 1).bind(Binding::primary(Key::O)),
        file("file.save").menu("file", 1, 0).bind(Binding::primary(Key::S)),
        file("file.save-as")
            .menu("file", 1, 1)
            .bind(Binding::primary(Key::S).shift()),
        file("file.close").menu("file", 1, 2).bind(Binding::primary(Key::W)),
        CommandSpec::new("file.preferences", Category::File).menu("file", 2, 0),
        file("file.quit").menu("file", 3, 0).bind(Binding::primary(Key::Q)),
    ]
}
