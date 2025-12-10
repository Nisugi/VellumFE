//! Menu keybind validator
//!
//! Validates that all critical menu actions have keybinds assigned
//! and checks for duplicate bindings.

use crate::config::MenuKeybinds;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub enum ValidationIssue {
    MissingCriticalBinding {
        action: String,
        field: String,
        default: String,
    },
    DuplicateBinding {
        keybind: String,
        actions: Vec<String>,
    },
}

impl ValidationIssue {
    pub fn severity(&self) -> ValidationSeverity {
        match self {
            ValidationIssue::MissingCriticalBinding { .. } => ValidationSeverity::Error,
            ValidationIssue::DuplicateBinding { .. } => ValidationSeverity::Warning,
        }
    }

    pub fn message(&self) -> String {
        match self {
            ValidationIssue::MissingCriticalBinding {
                action,
                field,
                default,
            } => {
                format!(
                    "Critical action '{}' has no keybind! Field '{}' is empty. Default: {}",
                    action, field, default
                )
            }
            ValidationIssue::DuplicateBinding { keybind, actions } => {
                format!(
                    "Keybind '{}' is assigned to multiple actions: {}",
                    keybind,
                    actions.join(", ")
                )
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationSeverity {
    Error,
    Warning,
}

pub struct ValidationResult {
    pub issues: Vec<ValidationIssue>,
}

impl ValidationResult {
    pub fn is_valid(&self) -> bool {
        !self.has_errors()
    }

    pub fn has_errors(&self) -> bool {
        self.issues
            .iter()
            .any(|i| matches!(i.severity(), ValidationSeverity::Error))
    }

    pub fn has_warnings(&self) -> bool {
        self.issues
            .iter()
            .any(|i| matches!(i.severity(), ValidationSeverity::Warning))
    }

    pub fn errors(&self) -> Vec<&ValidationIssue> {
        self.issues
            .iter()
            .filter(|i| matches!(i.severity(), ValidationSeverity::Error))
            .collect()
    }

    pub fn warnings(&self) -> Vec<&ValidationIssue> {
        self.issues
            .iter()
            .filter(|i| matches!(i.severity(), ValidationSeverity::Warning))
            .collect()
    }
}

/// Validate menu keybinds configuration
pub fn validate_menu_keybinds(keybinds: &MenuKeybinds) -> ValidationResult {
    let mut issues = Vec::new();

    // Check critical bindings (must not be empty)
    check_critical_binding(
        &mut issues,
        "Cancel (Esc)",
        "cancel",
        &keybinds.cancel,
        "Esc",
    );
    check_critical_binding(
        &mut issues,
        "Navigate Up",
        "navigate_up",
        &keybinds.navigate_up,
        "Up",
    );
    check_critical_binding(
        &mut issues,
        "Navigate Down",
        "navigate_down",
        &keybinds.navigate_down,
        "Down",
    );
    check_critical_binding(
        &mut issues,
        "Next Field",
        "next_field",
        &keybinds.next_field,
        "Tab",
    );
    check_critical_binding(
        &mut issues,
        "Previous Field",
        "previous_field",
        &keybinds.previous_field,
        "Shift+Tab",
    );

    // Check for duplicate bindings
    check_duplicates(&mut issues, keybinds);

    ValidationResult { issues }
}

/// Check if a critical binding is empty
fn check_critical_binding(
    issues: &mut Vec<ValidationIssue>,
    action: &str,
    field: &str,
    value: &str,
    default: &str,
) {
    if value.trim().is_empty() {
        issues.push(ValidationIssue::MissingCriticalBinding {
            action: action.to_string(),
            field: field.to_string(),
            default: default.to_string(),
        });
    }
}

/// Check for duplicate keybind assignments
fn check_duplicates(issues: &mut Vec<ValidationIssue>, keybinds: &MenuKeybinds) {
    let mut keybind_map: HashMap<String, Vec<String>> = HashMap::new();

    // Build map of keybind -> actions
    add_binding(&mut keybind_map, &keybinds.navigate_up, "navigate_up");
    add_binding(&mut keybind_map, &keybinds.navigate_down, "navigate_down");
    add_binding(&mut keybind_map, &keybinds.navigate_left, "navigate_left");
    add_binding(&mut keybind_map, &keybinds.navigate_right, "navigate_right");
    add_binding(&mut keybind_map, &keybinds.page_up, "page_up");
    add_binding(&mut keybind_map, &keybinds.page_down, "page_down");
    add_binding(&mut keybind_map, &keybinds.home, "home");
    add_binding(&mut keybind_map, &keybinds.end, "end");
    add_binding(&mut keybind_map, &keybinds.next_field, "next_field");
    add_binding(&mut keybind_map, &keybinds.previous_field, "previous_field");
    add_binding(&mut keybind_map, &keybinds.select, "select");
    add_binding(&mut keybind_map, &keybinds.cancel, "cancel");
    add_binding(&mut keybind_map, &keybinds.save, "save");
    add_binding(&mut keybind_map, &keybinds.delete, "delete");
    add_binding(&mut keybind_map, &keybinds.select_all, "select_all");
    add_binding(&mut keybind_map, &keybinds.copy, "copy");
    add_binding(&mut keybind_map, &keybinds.cut, "cut");
    add_binding(&mut keybind_map, &keybinds.paste, "paste");
    add_binding(&mut keybind_map, &keybinds.toggle, "toggle");
    add_binding(&mut keybind_map, &keybinds.move_up, "move_up");
    add_binding(&mut keybind_map, &keybinds.move_down, "move_down");
    add_binding(&mut keybind_map, &keybinds.add, "add");
    add_binding(&mut keybind_map, &keybinds.edit, "edit");

    // Find duplicates
    for (keybind, actions) in keybind_map.iter() {
        if actions.len() > 1 {
            issues.push(ValidationIssue::DuplicateBinding {
                keybind: keybind.clone(),
                actions: actions.clone(),
            });
        }
    }
}

fn add_binding(map: &mut HashMap<String, Vec<String>>, keybind: &str, action: &str) {
    if !keybind.trim().is_empty() {
        map.entry(keybind.to_string())
            .or_default()
            .push(action.to_string());
    }
}

/// Auto-fix validation issues by restoring defaults
pub fn auto_fix_menu_keybinds(keybinds: &mut MenuKeybinds, issues: &[ValidationIssue]) -> usize {
    let mut fixed_count = 0;

    for issue in issues {
        if let ValidationIssue::MissingCriticalBinding { field, default, .. } = issue {
            // Restore default value
            match field.as_str() {
                "cancel" => {
                    keybinds.cancel = default.clone();
                    fixed_count += 1;
                }
                "navigate_up" => {
                    keybinds.navigate_up = default.clone();
                    fixed_count += 1;
                }
                "navigate_down" => {
                    keybinds.navigate_down = default.clone();
                    fixed_count += 1;
                }
                "next_field" => {
                    keybinds.next_field = default.clone();
                    fixed_count += 1;
                }
                "previous_field" => {
                    keybinds.previous_field = default.clone();
                    fixed_count += 1;
                }
                _ => {}
            }
        }
    }

    fixed_count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_keybinds() {
        let keybinds = MenuKeybinds::default();
        let result = validate_menu_keybinds(&keybinds);
        assert!(result.is_valid());
        assert!(!result.has_errors());
    }

    #[test]
    fn test_missing_critical_binding() {
        let mut keybinds = MenuKeybinds::default();
        keybinds.cancel = String::new(); // Empty critical binding

        let result = validate_menu_keybinds(&keybinds);
        assert!(!result.is_valid());
        assert!(result.has_errors());
        assert_eq!(result.errors().len(), 1);
    }

    #[test]
    fn test_auto_fix() {
        let mut keybinds = MenuKeybinds::default();
        keybinds.cancel = String::new();
        keybinds.navigate_up = String::new();

        let result = validate_menu_keybinds(&keybinds);
        assert_eq!(result.errors().len(), 2);

        let fixed = auto_fix_menu_keybinds(&mut keybinds, &result.issues);
        assert_eq!(fixed, 2);
        assert_eq!(keybinds.cancel, "Esc");
        assert_eq!(keybinds.navigate_up, "Up");

        // Validate again - should be clean
        let result2 = validate_menu_keybinds(&keybinds);
        assert!(result2.is_valid());
    }
}
