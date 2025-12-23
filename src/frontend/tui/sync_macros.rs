//! Macros for reducing boilerplate in widget synchronization
//!
//! These macros provide a standard pattern for syncing simple widgets
//! where the widget type, storage field, and content variant follow a
//! consistent pattern.

/// Implements a basic widget sync function following the standard pattern:
/// - Iterate windows
/// - Match specific WindowContent variant
/// - Create widget if not exists
/// - Update widget from state
/// - Apply configuration (colors, borders, highlights, etc.)
///
/// This macro works for simple widgets that:
/// - Have a straightforward `update_from_state()` method
/// - Need standard config application (colors, borders, highlights)
/// - Don't require complex setup logic
///
/// # Example
///
/// ```rust,ignore
/// // In sync.rs:
/// impl TuiFrontend {
///     sync_simple_widget!(
///         sync_compass_widgets,
///         compass::Compass,
///         compass_widgets,
///         WindowContent::Empty,
///         WidgetType::Compass
///     );
/// }
/// ```
///
/// This generates a function equivalent to:
/// ```rust,ignore
/// pub(crate) fn sync_compass_widgets(
///     &mut self,
///     app_core: &crate::core::AppCore,
///     theme: &crate::theme::AppTheme,
/// ) {
///     for (name, window) in &app_core.ui_state.windows {
///         if matches!(window.content, WindowContent::Empty) && window.widget_type == WidgetType::Compass {
///             // Create widget if needed
///             if !self.widget_manager.compass_widgets.contains_key(name) {
///                 let mut widget = compass::Compass::new(name);
///                 let highlights: Vec<_> = app_core.config.highlights.values().cloned().collect();
///                 widget.set_highlights(highlights);
///                 widget.set_replace_enabled(app_core.config.highlight_settings.replace_enabled);
///                 self.widget_manager.compass_widgets.insert(name.clone(), widget);
///             }
///
///             // Update widget
///             if let Some(widget) = self.widget_manager.compass_widgets.get_mut(name) {
///                 // Apply config
///                 if let Some(template) = app_core.config.get_window_template(name) {
///                     widget.set_text_color(template.text_color.clone());
///                     widget.set_background_color(template.background_color.clone());
///                     widget.set_border_config(
///                         template.border.show,
///                         template.border.style.clone(),
///                         template.border.color.clone(),
///                     );
///                     widget.set_title(template.title.clone().unwrap_or_else(|| name.to_string()));
///                 }
///             }
///         }
///     }
/// }
/// ```
#[macro_export]
macro_rules! sync_simple_widget {
    (
        $func_name:ident,
        $widget_path:path,
        $storage_field:ident,
        $content_variant:pat,
        $widget_type:expr
    ) => {
        pub(crate) fn $func_name(
            &mut self,
            app_core: &crate::core::AppCore,
            theme: &crate::theme::AppTheme,
        ) {
            for (name, window) in &app_core.ui_state.windows {
                if matches!(window.content, $content_variant) && window.widget_type == $widget_type {
                    // Create widget if needed
                    if !self.widget_manager.$storage_field.contains_key(name) {
                        let mut widget = <$widget_path>::new(name);
                        let highlights: Vec<_> = app_core.config.highlights.values().cloned().collect();
                        widget.set_highlights(highlights);
                        widget.set_replace_enabled(app_core.config.highlight_settings.replace_enabled);
                        self.widget_manager.$storage_field.insert(name.clone(), widget);
                    }

                    // Update widget
                    if let Some(widget) = self.widget_manager.$storage_field.get_mut(name) {
                        // Apply config from window template
                        if let Some(template) = app_core.config.get_window_template(name) {
                            widget.set_text_color(template.text_color.clone());
                            widget.set_background_color(template.background_color.clone());
                            widget.set_border_config(
                                template.border.show,
                                template.border.style.clone(),
                                template.border.color.clone(),
                            );
                            widget.set_title(template.title.clone().unwrap_or_else(|| name.to_string()));
                        }
                    }
                }
            }
        }
    };
}
