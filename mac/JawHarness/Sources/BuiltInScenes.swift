/// The harness's table of contents: a scene's line here puts it in the sidebar, in this order within
/// its section. The scene itself is in a file of its own under `Scenes/`.
enum BuiltInScenes {
    @MainActor
    static func catalog() -> HarnessCatalog {
        var catalog = HarnessCatalog()
        catalog.register(.roles)
        catalog.register(.mimic)
        catalog.register(.chatList)
        catalog.register(.conversation)
        catalog.register(.palette)
        catalog.register(.states)
        return catalog
    }
}
