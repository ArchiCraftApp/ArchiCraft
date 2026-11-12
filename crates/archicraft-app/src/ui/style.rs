//! The application's text styles (UI-001): one size for the texts of the
//! toolbar, the properties strip, the view-control bar, the status line and
//! the project browser, which show their labels, buttons and numbers in the
//! Body, Button and Monospace styles. The small style stays at egui's size
//! for secondary text inside dialogs; the bars do not use it.

use egui::{Context, Style, TextStyle};

/// The size in points of the Body, Button and Monospace text styles, one
/// point above egui's 13 so that the bars read well (UI-001).
pub const TEXT_SIZE: f32 = 14.0;

/// Sets the text styles of both themes on `ctx` (UI-001). The application
/// calls it where it sets the theme, so the styles hold from the first
/// frame whichever theme is chosen.
pub fn apply(ctx: &Context) {
    ctx.all_styles_mut(set_text_styles);
}

/// Sets the Body, Button and Monospace styles of `style` to
/// [`TEXT_SIZE`], keeping their font families.
pub fn set_text_styles(style: &mut Style) {
    for (text_style, font) in &mut style.text_styles {
        if matches!(text_style, TextStyle::Body | TextStyle::Button | TextStyle::Monospace) {
            font.size = TEXT_SIZE;
        }
    }
}

#[cfg(test)]
mod tests {
    use egui::{FontFamily, Theme};

    use super::*;

    /// UI-001: after `apply`, both themes show Body, Button and Monospace
    /// at one size of at least 14 pt; Monospace keeps its family.
    #[test]
    fn apply_gives_the_bar_styles_one_size() {
        let ctx = Context::default();
        apply(&ctx);
        for theme in [Theme::Dark, Theme::Light] {
            let style = ctx.style_of(theme);
            let font = |s: TextStyle| style.text_styles[&s].clone();
            let sizes = [TextStyle::Body, TextStyle::Button, TextStyle::Monospace].map(|s| font(s).size);
            assert!(sizes.iter().all(|&s| s == sizes[0]), "{theme:?}: {sizes:?}");
            assert!(sizes[0] >= 14.0, "{theme:?}: {sizes:?}");
            assert_eq!(font(TextStyle::Monospace).family, FontFamily::Monospace);
        }
    }

    /// UI-001: the sources of the status line and toolbar (`ui/shell.rs`),
    /// the view-control bar, the project browser, the properties strip and
    /// the bar buttons (`command_widgets::bar_button`) set no small or own
    /// text size, so their texts take the one size of [`TEXT_SIZE`].
    #[test]
    fn the_bars_use_no_small_or_own_text_size() {
        let widgets = include_str!("command_widgets.rs").replace("\r\n", "\n");
        let start = widgets.find("\npub fn bar_button(").expect("fn bar_button");
        let end = widgets[start..].find("\n}\n").map_or(widgets.len(), |e| start + e + 3);
        let sources = [
            ("ui/shell.rs", include_str!("shell.rs")),
            ("ui/view_bar.rs", include_str!("view_bar.rs")),
            ("ui/browser.rs", include_str!("browser.rs")),
            ("ui/properties.rs", include_str!("properties.rs")),
            ("ui/strip.rs", include_str!("strip.rs")),
            ("command_widgets::bar_button", &widgets[start..end]),
        ];
        for (name, source) in sources {
            let source: String = source.split_whitespace().collect();
            assert!(!source.contains(".small()"), "{name} uses .small()");
            assert!(!source.contains("small_button("), "{name} uses small_button");
            assert!(!source.contains("RichText::size("), "{name} uses RichText::size");
            // A text size set on a RichText: `.size(` with an argument, which
            // leaves out `rect.size()` and the like.
            assert!(
                source
                    .match_indices(".size(")
                    .all(|(i, _)| source[i..].starts_with(".size()")),
                "{name} sets a text size"
            );
        }
    }
}
