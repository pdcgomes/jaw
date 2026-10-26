import AppKit
import JawThemes

/// The window's one box, under the panes and as wide as them: the focused chat's composer, or,
/// when the focused pane has no chat, a box that says how to give it one. Each chat keeps its own
/// composer, with its draft, its reply or edit and what didn't send, so moving the focus loses
/// nothing typed.
@MainActor
public final class BoxPlace: NSView {
    var shown: NSView?
    let none = Surface(.field, radius: Radius.field)
    let noneSays = Labels.make("this pane has no chat: ⌘K to put one here", .body)

    override public init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        noneSays.translatesAutoresizingMaskIntoConstraints = false
        none.addSubview(noneSays)
        NSLayoutConstraint.activate([
            noneSays.leadingAnchor.constraint(equalTo: none.leadingAnchor, constant: Space.l),
            noneSays.centerYAnchor.constraint(equalTo: none.centerYAnchor),
            none.heightAnchor.constraint(equalToConstant: 30),
        ])
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    /// Shows `composer`'s view, or the box for an empty pane.
    public func show(_ composer: NSView?, roles: Roles) {
        layer?.backgroundColor = roles.ground.cgColor
        none.roles = roles
        Labels.style(noneSays, .tertiary, roles: roles)
        let view = composer ?? none
        guard view !== shown else { return }
        shown?.removeFromSuperview()
        view.removeFromSuperview()
        view.translatesAutoresizingMaskIntoConstraints = false
        addSubview(view)
        let inset = view === none ? Space.l : 0
        NSLayoutConstraint.activate([
            view.topAnchor.constraint(equalTo: topAnchor, constant: view === none ? Space.m : 0),
            view.bottomAnchor.constraint(equalTo: bottomAnchor, constant: view === none ? -Space.l + 2 : 0),
            view.leadingAnchor.constraint(equalTo: leadingAnchor, constant: inset),
            view.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -inset),
        ])
        shown = view
    }
}
