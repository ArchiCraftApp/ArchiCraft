//! Commands of the File menu (UI-011, PRJ-003, PRJ-004).

use egui::Key;

use super::{Binding, Category, CommandId, CommandSpec, Handler, Scope};
use crate::app::Then;

/// Open…, which the File menu follows with the Open recent submenu.
pub const OPEN: CommandId = CommandId::new("file.open");

pub(super) fn specs() -> Vec<CommandSpec> {
    let file = |id, handler: Handler| CommandSpec::new(id, Category::File, handler).scope(Scope::TextSafe);
    vec![
        file("file.new", |app, ctx, _| app.guard(ctx, Then::New))
            .menu("file", 0, 0)
            .bind(Binding::primary(Key::N)),
        file(OPEN.as_str(), |app, ctx, _| app.guard(ctx, Then::Open(None)))
            .menu("file", 0, 1)
            .bind(Binding::primary(Key::O)),
        file("file.save", |app, _, _| {
            app.save(false);
        })
        .menu("file", 1, 0)
        .bind(Binding::primary(Key::S)),
        file("file.save-as", |app, _, _| {
            app.save(true);
        })
        .menu("file", 1, 1)
        .bind(Binding::primary(Key::S).shift()),
        file("file.close", |app, ctx, _| app.guard(ctx, Then::Close))
            .menu("file", 1, 2)
            .bind(Binding::primary(Key::W)),
        CommandSpec::new("file.preferences", Category::File, |app, _, _| {
            app.dialogs.preferences = Some(app.ws.prefs.clone());
        })
        .menu("file", 2, 0),
        file("file.quit", |app, ctx, _| app.guard(ctx, Then::Quit))
            .menu("file", 3, 0)
            .bind(Binding::primary(Key::Q)),
    ]
}
