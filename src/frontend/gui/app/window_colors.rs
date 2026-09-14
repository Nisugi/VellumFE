//! Per-window color resolution for the GUI: a thin wrapper over the shared
//! core resolver (`core/window_style.rs`), supplying the tab's catalog
//! window definition (or `None` for ephemeral panels with no def, which
//! fall through to theme defaults) and the GUI's own hex-name parser.

use super::*;
use crate::core::window_style::{resolve_window_colors, WindowColors};

/// Resolve colors for an optional window def, precedence: the def, else a
/// colors.toml `ui.*` value the user actually changed, else the theme.
/// `None` (no catalog def — ephemeral panels) resolves as if every def
/// field were unset, i.e. straight to the user/theme layers. Pure and
/// `VellumGuiApp`-free so it's directly unit-testable (see `tests` below).
pub(super) fn resolve_colors_for_base(
    base: Option<&crate::config::WindowBase>,
    ui: &crate::config::UiColors,
    theme: &crate::theme::AppTheme,
) -> WindowColors {
    // WindowBase has no Default (hand-rolled serde shadow); an
    // all-fields-unset instance built the same way window_style.rs's own
    // tests do stands in for "no catalog def" so every resolver layer
    // above the def (user ui.*, theme) still applies.
    let empty_base: crate::config::WindowBase;
    let base = match base {
        Some(base) => base,
        None => {
            empty_base = toml::from_str("name = \"\"").expect("minimal WindowBase from empty toml");
            &empty_base
        }
    };
    resolve_window_colors(
        base,
        ui,
        theme,
        crate::frontend::common::color::parse_color_flexible,
    )
}

impl VellumGuiApp {
    /// Resolved border/background/text colors for a tab's window. See
    /// `resolve_colors_for_base` for precedence.
    ///
    /// Uncached: a handful of string parses per window per frame, same
    /// order of cost as the border/background lookups already done per
    /// frame elsewhere in this file (`accent_color_for_tab`,
    /// `widget_render_settings`'s background lookup). Revisit only if
    /// profiling shows it hot.
    pub(super) fn resolved_window_colors(&self, tab: &GuiTab) -> WindowColors {
        let def_base = self
            .app_core
            .layout
            .windows
            .iter()
            .find(|window| window.name() == tab.window_name)
            .map(|window| window.base());
        resolve_colors_for_base(
            def_base,
            &self.app_core.config.colors.ui,
            &self.current_theme,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{UiColors, WindowBase};
    use crate::theme::AppTheme;

    fn base_with_text(text: &str) -> WindowBase {
        let mut base: WindowBase = toml::from_str("name = \"test\"").expect("minimal WindowBase");
        base.text_color = Some(text.to_string());
        base
    }

    #[test]
    fn def_text_color_wins() {
        let base = base_with_text("#112233");
        let colors =
            resolve_colors_for_base(Some(&base), &UiColors::default(), &AppTheme::default());
        assert_eq!(colors.text.as_deref(), Some("#112233"));
    }

    #[test]
    fn no_def_falls_through_to_theme_text_primary() {
        let theme = AppTheme::default();
        let colors = resolve_colors_for_base(None, &UiColors::default(), &theme);
        let expected = format!(
            "#{:02x}{:02x}{:02x}",
            theme.text_primary.r, theme.text_primary.g, theme.text_primary.b
        );
        assert_eq!(colors.text.as_deref(), Some(expected.as_str()));
    }

    #[test]
    fn transparent_background_def_yields_none() {
        let mut base: WindowBase = toml::from_str("name = \"test\"").expect("minimal WindowBase");
        base.transparent_background = true;
        base.background_color = Some("#112233".to_string());
        let colors =
            resolve_colors_for_base(Some(&base), &UiColors::default(), &AppTheme::default());
        assert_eq!(colors.background, None);
    }
}
