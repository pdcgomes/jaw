import AppKit
import JawKit

public extension SidebarController {
    /// A folder being named in place: a new one, in `parent` or at the top, with what to file in it once
    /// it's made; or one of yours being renamed. What's typed, and jawd's refusal of the name last
    /// asked for, which is said under the field.
    struct Naming: Equatable, Sendable {
        public var parent: String?
        public var renaming: String?
        public var filing: Filed?
        public var text: String
        public var refused: String?
        /// Asked of jawd and not answered yet, so Return doesn't ask again meanwhile.
        public var asked = false
        /// jawd turned down what's typed while the keys were elsewhere, so giving the field up again
        /// drops it rather than asking for it again; typing clears it.
        public var away = false
    }
}

extension SidebarController: NSTextFieldDelegate {
    /// New Folder: a row with its name field open where the folder will go, last in `parent` or at the
    /// top, the folders it's in opened; `filing` is filed in it once it's made.
    public func beginNaming(parent: String?, filing: Filed? = nil, text: String = "") {
        guard tree.served, isViewLoaded else { return }
        keepingField = true
        naming = Naming(parent: parent, renaming: nil, filing: filing, text: text, refused: nil)
        outline.reloadData()
        restoreOpen()
        if let parent { expand(parent) } else { outline.expandItem(item(Self.foldersHeading)) }
        keepingField = false
        focusField()
    }

    /// Rename: the same field, on a folder of yours, holding its name, the folders it's in opened.
    public func beginRenaming(_ folder: String) {
        guard let row = tree.row(folder), row.isYours, isViewLoaded else { return }
        keepingField = true
        naming = Naming(parent: row.parent, renaming: folder, filing: nil, text: row.name, refused: nil)
        outline.reloadData()
        restoreOpen()
        for id in above(folder) {
            outline.expandItem(item(id))
        }
        keepingField = false
        focusField()
    }

    /// Esc, or a name given up empty, unchanged or as jawd last turned it down: the folder isn't made,
    /// or keeps its name. Nothing while jawd is answering for the name, as a click away does nothing
    /// then, since the folder may be made by then, and a chat's New Folder… files the chat in it once
    /// it is.
    public func cancelNaming() {
        guard let n = naming, !n.asked else { return }
        naming = nil
        outline.reloadData()
        restoreOpen()
    }

    /// jawd made the folder or renamed it: the field closes, and a folder made is there at once, before
    /// the tree's push brings it too. One made from the sidebar, with nothing to file in it, is picked,
    /// as Notes and Reminders select a new item in their sidebars; one made from a chat's Add to Folder
    /// leaves the list where it was.
    public func named(_ row: FolderRow?) {
        let picks = naming.map { $0.renaming == nil && $0.filing == nil } ?? false
        naming = nil
        if let row {
            tree.apply(Update(upsert: [row]))
            roster.tree = tree
            shapeChanges += 1
        }
        outline.reloadData()
        restoreOpen()
        guard let row, picks else { return }
        picked = row.id
        reselect()
        host?.sidebar(self, pick: row.id)
    }

    /// jawd turned the name down: its words go under the field, which keeps what's typed, and the keys
    /// if it has them; keys gone elsewhere aren't taken back. VoiceOver says them as they come, and
    /// they're the field's help after.
    public func notNamed(_ why: String) {
        guard var n = naming else { return }
        n.refused = why
        n.asked = false
        n.away = !typingName
        naming = n
        announce(why)
        let row = outline.row(forItem: items[n.renaming ?? Self.namingRow])
        guard row >= 0, let cell = outline.view(atColumn: 0, row: row, makeIfNecessary: false) as? NamingCell else { return }
        cell.show(n, delegate: self, roles: roles)
        // At once, as AppKit would otherwise ease the row to its height.
        NSAnimationContext.runAnimationGroup { context in
            context.duration = 0
            outline.noteHeightOfRows(withIndexesChanged: [row])
        }
    }

    /// Return: asks the host for the name typed, unless jawd hasn't answered for the last.
    func commitNaming() {
        guard var n = naming, !n.asked else { return }
        n.asked = true
        naming = n
        host?.sidebar(self, name: n.text, as: n)
    }

    /// Gives the field the keys, its text all selected, or `selection`, as it was before the tree was
    /// laid out again.
    func focusField(_ selection: NSRange? = nil) {
        let row = outline.row(forItem: items[naming?.renaming ?? Self.namingRow])
        guard row >= 0, let cell = outline.view(atColumn: 0, row: row, makeIfNecessary: true) as? NamingCell else { return }
        outline.scrollRowToVisible(row)
        keepingField = true
        view.window?.makeFirstResponder(cell.field)
        keepingField = false
        if let selection {
            cell.field.currentEditor()?.selectedRange = selection
        } else {
            cell.field.currentEditor()?.selectAll(nil)
        }
    }

    /// Whether the name field has the keys.
    var typingName: Bool {
        naming != nil && ((view.window?.firstResponder as? NSTextView)?.delegate as? NSTextField)?.delegate === self
    }

    /// Lays the tree out again without giving up a name being typed: the field is drawn again with
    /// what's typed and, when it had the keys, given them back with its selection as it was.
    func keepingNaming(_ change: () -> Void) {
        let selection = typingName ? (view.window?.firstResponder as? NSTextView)?.selectedRange() : nil
        keepingField = true
        change()
        keepingField = false
        if let selection { focusField(selection) }
    }

    public func controlTextDidChange(_ note: Notification) {
        guard let field = note.object as? NSTextField else { return }
        naming?.text = field.stringValue
        naming?.away = false
    }

    public func control(_: NSControl, textView _: NSTextView, doCommandBy selector: Selector) -> Bool {
        switch selector {
        case #selector(NSResponder.insertNewline(_:)):
            commitNaming()
            return true
        case #selector(NSResponder.cancelOperation(_:)):
            cancelNaming()
            return true
        default:
            return false
        }
    }

    /// The field given up other than by Return or Esc, as a click elsewhere gives it up: the name typed
    /// is asked for as Return asks, as Finder, Notes and Xcode keep a name, unless it's empty, unchanged
    /// or what jawd turned down while the keys were elsewhere, which drop it as Esc does. Nothing while
    /// jawd is answering for it.
    public func controlTextDidEndEditing(_: Notification) {
        guard !keepingField, let n = naming, !n.asked else { return }
        let text = n.text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty, text != n.renaming.flatMap({ tree.row($0)?.name }), !n.away else { return cancelNaming() }
        commitNaming()
    }
}
