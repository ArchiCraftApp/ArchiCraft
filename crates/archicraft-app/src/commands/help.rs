//! Commands of the Help menu.

use super::{Category, CommandSpec, Handler};

pub(super) fn specs() -> Vec<CommandSpec> {
    let help = |id, order, handler: Handler| CommandSpec::new(id, Category::Help, handler).menu("help", 0, order);
    vec![
        help("help.shortcuts", 0, |app, _, _| app.dialogs.shortcuts = true),
        help("help.about", 1, |app, _, _| app.dialogs.about = true),
    ]
}
