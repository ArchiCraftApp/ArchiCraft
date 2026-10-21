//! Commands of the Help menu.

use super::{Category, CommandSpec};

pub(super) fn specs() -> Vec<CommandSpec> {
    let help = |id, order| CommandSpec::new(id, Category::Help).menu("help", 0, order);
    vec![help("help.shortcuts", 0), help("help.about", 1)]
}
