import AppKit

/// The window's questions about its layouts, as alerts it shows as sheets: a name to save one as or
/// rename it to, and whether to delete one.
@MainActor
public enum Prompts {
    /// An alert asking for a name, and its field, which takes the keys at once: Return answers with
    /// `button`, Esc cancels.
    public static func name(_ title: String, _ detail: String, value: String = "", button: String) -> (NSAlert, NSTextField) {
        let alert = NSAlert()
        alert.messageText = title
        alert.informativeText = detail
        let field = NSTextField(string: value)
        field.placeholderString = "Work"
        field.frame = NSRect(x: 0, y: 0, width: 260, height: 24)
        alert.accessoryView = field
        alert.addButton(withTitle: button)
        alert.addButton(withTitle: "Cancel")
        alert.window.initialFirstResponder = field
        return (alert, field)
    }

    /// An alert asking whether to delete the layout `name`, its Delete marked destructive.
    public static func deleteLayout(_ name: String) -> NSAlert {
        let alert = NSAlert()
        alert.messageText = "Delete \(name)?"
        alert.informativeText = "Its panes stay on screen, unsaved."
        alert.addButton(withTitle: "Delete").hasDestructiveAction = true
        alert.addButton(withTitle: "Cancel")
        return alert
    }
}
