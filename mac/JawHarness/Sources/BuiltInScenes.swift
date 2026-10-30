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
        catalog.register(.linkPreviews)
        catalog.register(.palette)
        catalog.register(.states)
        catalog.register(.screensJawd)
        catalog.register(.screensConnection)
        catalog.register(.screensLinking)
        catalog.register(.screensEmpty)
        catalog.register(.screensChat)
        catalog.register(.screensPanes)
        catalog.register(.screensFocus)
        catalog.register(.screensFolded)
        catalog.register(.screensOverlays)
        return catalog
    }
}
