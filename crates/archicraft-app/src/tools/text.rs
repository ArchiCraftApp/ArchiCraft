//! The text tool and its inline editor (ANN-007).
//!
//! A click with the text tool places a new text's anchor and opens the
//! inline editor there; a double-click on a text opens the editor on that
//! text. The plan view draws the editor and ends it: Ctrl/Cmd+Enter or a
//! click outside commits the content as one undo step, Esc cancels. Typing
//! goes to the editor's text field, so single-key shortcuts stay off while
//! it has focus (UI-008).

use std::sync::atomic::{AtomicU64, Ordering};

use archicraft_model::glam::DVec2;
use archicraft_model::{Command, Element, ElementId, Project, StoreyId};

/// The openings of inline editors so far, which number the next one.
static OPENINGS: AtomicU64 = AtomicU64::new(0);

/// The inline editor of a text (ANN-007): the text it edits and the content
/// typed so far.
#[derive(Clone, Debug, PartialEq)]
pub struct TextEditor {
    /// The working copy: a new text from the tool defaults, or a copy of
    /// the edited text as it was when the editor opened.
    pub text: Element,
    /// Whether the editor edits an existing text rather than a new one.
    pub existing: bool,
    /// The content typed so far; each `\n` starts a new line.
    pub content: String,
    /// Whether the editor's field still has to take the keyboard focus,
    /// set when it opens.
    pub wants_focus: bool,
    /// Which opening of an editor this is, unique while the application
    /// runs. The plan view keeps the field's state (cursor, undo, focus)
    /// per opening, so that an editor opened right after another one, even
    /// on the same text, starts afresh.
    pub opening: u64,
}

impl TextEditor {
    /// The editor of a new text on `storey` with its anchor at `anchor`,
    /// from the text tool's defaults, with no content yet.
    pub fn new(project: &Project, storey: StoreyId, anchor: DVec2) -> Self {
        Self {
            text: project.new_text(storey, anchor, "", 0),
            existing: false,
            content: String::new(),
            wants_focus: true,
            opening: OPENINGS.fetch_add(1, Ordering::Relaxed),
        }
    }

    /// The editor of the text `element` with its content; `None` if it is
    /// not a text.
    pub fn edit(element: &Element) -> Option<Self> {
        let content = element.as_text()?.content.clone();
        Some(Self {
            text: element.clone(),
            existing: true,
            content,
            wants_focus: true,
            opening: OPENINGS.fetch_add(1, Ordering::Relaxed),
        })
    }

    /// The storey of the edited text.
    pub fn storey(&self) -> StoreyId {
        self.text.storey
    }

    /// The edited text's id; a new text gets it once committed.
    pub fn element(&self) -> ElementId {
        self.text.id
    }

    /// The command that commits the editor, one undo step (ANN-007): a new
    /// text with the content, or the edited text with its content replaced,
    /// without trailing white space. Blank content creates nothing and
    /// leaves an edited text as it was, as does unchanged content: `None`.
    pub fn command(&self) -> Option<Command> {
        let content = self.content.trim_end();
        let before = self.text.as_text()?;
        if content.trim().is_empty() || (self.existing && content == before.content) {
            return None;
        }
        let mut text = self.text.clone();
        text.as_text_mut()?.content = content.to_owned();
        Some(if self.existing {
            Command::UpdateElements(vec![text])
        } else {
            Command::AddElements(vec![text])
        })
    }
}

#[cfg(test)]
mod tests {
    use archicraft_model::History;
    use archicraft_model::template::default_metric_project;

    use super::*;

    /// ANN-007: a new text's editor creates the text with its content and
    /// anchor as one undo step; blank content creates nothing.
    #[test]
    fn a_new_text_is_created_with_its_content_and_blank_creates_nothing() {
        let mut project = default_metric_project();
        let storey = project.storeys.ground();
        let anchor = DVec2::new(2.0, 3.0);
        let mut editor = TextEditor::new(&project, storey, anchor);
        assert_eq!(editor.command(), None, "nothing typed");
        editor.content = " \n  ".into();
        assert_eq!(editor.command(), None, "blank");
        editor.content = "First\nSecond\n".into();
        let command = editor.command().expect("a text");
        let mut history = History::default();
        history.execute(&mut project, command).unwrap();
        assert_eq!(history.undo_len(), 1);
        let text = project.elements[&editor.element()].as_text().unwrap();
        assert_eq!(text.content, "First\nSecond", "trailing white space dropped");
        assert_eq!(text.anchor, anchor);
    }

    /// ANN-007: an existing text's editor replaces its content as one undo
    /// step and keeps everything else; unchanged or blank content changes
    /// nothing.
    #[test]
    fn an_edited_text_takes_the_new_content() {
        let mut project = default_metric_project();
        let storey = project.storeys.ground();
        let mut history = History::default();
        let text = project.new_text(storey, DVec2::ZERO, "Note", 0);
        let id = text.id;
        history.execute(&mut project, Command::AddElements(vec![text])).unwrap();
        let mut editor = TextEditor::edit(&project.elements[&id]).unwrap();
        assert_eq!(editor.content, "Note");
        assert_eq!(editor.command(), None, "unchanged");
        editor.content = "  ".into();
        assert_eq!(editor.command(), None, "blank keeps the text");
        editor.content = "Note\nmore".into();
        let Some(Command::UpdateElements(updated)) = editor.command() else {
            panic!("an update")
        };
        let mut expected = project.elements[&id].clone();
        expected.as_text_mut().unwrap().content = "Note\nmore".into();
        assert_eq!(updated, [expected]);
    }
}
