/// The harness's table of contents: a scene's line here puts it in the sidebar, in this order within
/// its section. The scene itself is in a file of its own under `Scenes/`.
enum BuiltInScenes {
    @MainActor
    static func catalog() -> HarnessCatalog {
        var catalog = HarnessCatalog()
        catalog.register(.roles)
        catalog.register(.mimic)
        catalog.register(.chatList)
        catalog.register(.sidebar)
        catalog.register(.emptyFolder)
        catalog.register(.roster)
        catalog.register(.conversation)
        catalog.register(.linkPreviews)
        catalog.register(.polls)
        catalog.register(.pictures)
        catalog.register(.downloadStates)
        catalog.register(.contactCards)
        catalog.register(.libraryGrid)
        catalog.register(.libraryRows)
        catalog.register(.libraryAll)
        catalog.register(.libraryKinds)
        catalog.register(.librarySearch)
        catalog.register(.palette)
        catalog.register(.pollForm)
        catalog.register(.contactSheet)
        catalog.register(.states)
        catalog.register(.screensJawd)
        catalog.register(.screensConnection)
        catalog.register(.screensLinking)
        catalog.register(.screensEmpty)
        catalog.register(.screensChat)
        catalog.register(.screensPanes)
        catalog.register(.screensLibrary)
        catalog.register(.screensFocus)
        catalog.register(.screensFolded)
        catalog.register(.screensOverlays)
        return catalog
    }
}
