//! Behavior traits for menu widgets
//!
//! These traits define common behaviors that widgets can implement,
//! enabling code reuse and consistent patterns across all menu widgets.

use anyhow::Result;
use tui_textarea::TextArea;

/// Trait for widgets that support list navigation
pub trait Navigable {
    /// Move selection up one item
    fn navigate_up(&mut self);

    /// Move selection down one item
    fn navigate_down(&mut self);

    /// Move selection up one page (~10 items)
    fn page_up(&mut self);

    /// Move selection down one page (~10 items)
    fn page_down(&mut self);

    /// Move to first item (optional - not all widgets support this)
    fn home(&mut self) {}

    /// Move to last item (optional - not all widgets support this)
    fn end(&mut self) {}
}

/// Trait for widgets that have selectable items (browsers)
pub trait Selectable {
    /// Get the currently selected item's key/name
    fn get_selected(&self) -> Option<String>;

    /// Delete the currently selected item
    /// Returns the key of the deleted item if successful
    fn delete_selected(&mut self) -> Option<String>;
}

/// Trait for widgets with text input fields (forms, editors)
pub trait TextEditable {
    /// Get reference to the currently focused text field
    fn get_focused_field(&self) -> Option<&TextArea<'static>>;

    /// Get mutable reference to the currently focused text field
    fn get_focused_field_mut(&mut self) -> Option<&mut TextArea<'static>>;

    /// Select all text in the focused field (Ctrl+A)
    fn select_all(&mut self) {
        if let Some(field) = self.get_focused_field_mut() {
            // Move to end, start selection, move to beginning
            field.move_cursor(tui_textarea::CursorMove::End);
            field.start_selection();
            field.move_cursor(tui_textarea::CursorMove::Head);
        }
    }

    /// Copy selected text to clipboard (Ctrl+C)
    fn copy_to_clipboard(&self) -> Result<()> {
        if let Some(field) = self.get_focused_field() {
            let selected = field.yank_text();
            if !selected.is_empty() {
                crate::clipboard::copy(&selected)?;
            }
        }
        Ok(())
    }

    /// Cut selected text to clipboard (Ctrl+X)
    fn cut_to_clipboard(&mut self) -> Result<()> {
        if let Some(field) = self.get_focused_field() {
            let selected = field.yank_text();
            if !selected.is_empty() {
                crate::clipboard::cut(&selected)?;
            }
        }
        // Delete the selected text by inserting empty string (replaces selection)
        if let Some(field) = self.get_focused_field_mut() {
            field.insert_str("");
        }
        Ok(())
    }

    /// Paste text from clipboard (Ctrl+V)
    fn paste_from_clipboard(&mut self) -> Result<()> {
        let text = crate::clipboard::paste()?;
        if let Some(field) = self.get_focused_field_mut() {
            field.insert_str(&text);
        }
        Ok(())
    }
}

/// Trait for widgets with boolean toggles (checkboxes)
pub trait Toggleable {
    /// Toggle the focused boolean field
    /// Returns the new value if successful
    fn toggle_focused(&mut self) -> Option<bool>;
}

/// Trait for widgets with dropdowns/enums that can be cycled
pub trait Cyclable {
    /// Cycle to next value (Down arrow or Space)
    fn cycle_forward(&mut self);

    /// Cycle to previous value (Up arrow)
    fn cycle_backward(&mut self);
}

/// Trait for widgets that support field navigation (forms)
pub trait FieldNavigable {
    /// Move to next field (Tab)
    fn next_field(&mut self);

    /// Move to previous field (Shift+Tab)
    fn previous_field(&mut self);

    /// Get the number of fields
    fn field_count(&self) -> usize;

    /// Get the current field index
    fn current_field(&self) -> usize;
}

/// Trait for widgets that can be saved (forms, editors)
/// Uses associated type to allow each form to return its own rich result type
pub trait Saveable {
    /// The result type returned by try_save (e.g., FormResult, KeybindFormResult, etc.)
    type SaveResult;

    /// Attempt to save the current state
    /// Returns Some(result) if save was attempted, None if validation failed
    fn try_save(&mut self) -> Option<Self::SaveResult>;

    /// Check if the widget has been modified
    fn is_modified(&self) -> bool {
        true // Default: always consider modified
    }
}

/// Trait for widgets that support item reordering (WindowEditor tabs/indicators)
pub trait Reorderable {
    /// Move selected item up in the list
    fn move_item_up(&mut self) -> bool;

    /// Move selected item down in the list
    fn move_item_down(&mut self) -> bool;
}

/// Trait for widgets that support adding/editing list items (WindowEditor)
pub trait ListManageable {
    /// Add a new item
    fn add_item(&mut self);

    /// Edit the selected item
    fn edit_item(&mut self);

    /// Delete the selected item
    fn delete_item(&mut self) -> Option<String>;
}

// BrowserNavigable trait removed - browsers now implement Navigable + Selectable directly
