import AppKit
import JawKit
import JawThemes
import SwiftUI

/// The panes' place: the panes with the focused chat's box under them, or what's there instead, a
/// state or linking.
@MainActor
public final class DetailController: NSViewController {
    public let panes = GridView()
    public let box = BoxPlace()
    /// The panes with the box under them.
    let panesArea = NSView()
    var stateHost: NSHostingController<StateView>?
    var linkingHost: NSHostingController<LinkingView>?
    var shown: NSView?

    override public func loadView() {
        view = NSView()
        for v in [panes, box] as [NSView] {
            v.translatesAutoresizingMaskIntoConstraints = false
            panesArea.addSubview(v)
        }
        NSLayoutConstraint.activate([
            panes.topAnchor.constraint(equalTo: panesArea.topAnchor),
            panes.leadingAnchor.constraint(equalTo: panesArea.leadingAnchor),
            panes.trailingAnchor.constraint(equalTo: panesArea.trailingAnchor),
            panes.bottomAnchor.constraint(equalTo: box.topAnchor),
            box.leadingAnchor.constraint(equalTo: panesArea.leadingAnchor),
            box.trailingAnchor.constraint(equalTo: panesArea.trailingAnchor),
            box.bottomAnchor.constraint(equalTo: panesArea.bottomAnchor),
        ])
    }

    var drawn: (PaneGrid, Int?, [Int: ObjectIdentifier], ObjectIdentifier?, Roles, PaneLook, [String: String], String?)?

    /// Lays the panes out with the focused chat's box under them, unless they're already so, with what
    /// `unopened` says for a slot whose chat won't have a pane.
    public func show(panes grid: PaneGrid, views: [Int: NSView], unopened: [String: String] = [:], focused: Int?,
                     box composer: NSView?, boxSays: String? = nil, roles: Roles, look: PaneLook, force: Bool = false)
    {
        let ids = views.mapValues(ObjectIdentifier.init)
        let boxID = composer.map(ObjectIdentifier.init)
        if !force, shown === panesArea, let drawn, drawn.0 == grid, drawn.1 == focused, drawn.2 == ids, drawn.3 == boxID, drawn.4 == roles,
           drawn.5 == look, drawn.6 == unopened, drawn.7 == boxSays
        {
            return
        }
        place(panesArea)
        panes.show(grid, views: views, unopened: unopened, focused: focused, roles: roles, look: look)
        box.show(composer, says: boxSays, roles: roles)
        drawn = (grid, focused, ids, boxID, roles, look, unopened, boxSays)
    }

    public func show(state kind: StateView.Kind, roles: Roles) {
        let view = StateView(kind: kind, roles: roles)
        if let host = stateHost {
            host.rootView = view
        } else {
            let host = NSHostingController(rootView: view)
            host.sizingOptions = []
            stateHost = host
        }
        place(stateHost!.view)
    }

    public func showLinking(step: LinkStep?, loggedOut: String?, roles: Roles, start: @escaping (String?) -> Void, cancel: @escaping () -> Void) {
        let view = LinkingView(step: step, loggedOut: loggedOut, roles: roles, start: start, cancel: cancel)
        if let host = linkingHost {
            host.rootView = view
        } else {
            let host = NSHostingController(rootView: view)
            host.sizingOptions = []
            linkingHost = host
        }
        place(linkingHost!.view)
    }

    func place(_ v: NSView) {
        guard shown !== v else { return }
        shown?.removeFromSuperview()
        v.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(v)
        NSLayoutConstraint.activate([
            v.topAnchor.constraint(equalTo: view.topAnchor),
            v.bottomAnchor.constraint(equalTo: view.bottomAnchor),
            v.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            v.trailingAnchor.constraint(equalTo: view.trailingAnchor),
        ])
        shown = v
        if v !== linkingHost?.view { linkingHost = nil }
    }
}
