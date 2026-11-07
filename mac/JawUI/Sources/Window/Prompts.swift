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

    /// Log Out…'s question, as `jaw unlink` asks it: only the phone can link Jaw again, and jawd keeps
    /// the messages, as `link.logout` does. `phone` is the account's number, when jawd said it.
    public static func logOut(_ phone: String?) -> NSAlert {
        let alert = NSAlert()
        alert.messageText = "Log Out of WhatsApp?"
        let whose = phone.map { "one of \($0)'s" } ?? "one of your"
        alert.informativeText = "Jaw stops being \(whose) linked devices, and only the phone can link it again. jawd keeps your messages."
        alert.addButton(withTitle: "Log Out").hasDestructiveAction = true
        alert.addButton(withTitle: "Cancel")
        return alert
    }

    /// Delete…'s question for a folder of yours: its chats stay wherever else they're filed, and its
    /// subfolders take its place, as jawd's `folder.delete` does.
    public static func deleteFolder(_ name: String) -> NSAlert {
        let alert = NSAlert()
        alert.messageText = "Delete \(name)?"
        alert.informativeText = "Its chats stay wherever else they're filed, and its subfolders take its place."
        alert.addButton(withTitle: "Delete").hasDestructiveAction = true
        alert.addButton(withTitle: "Cancel")
        return alert
    }
}
