import Foundation
@testable import JawKit
@testable import JawUI
import Testing

/// `find`'s links in the palette: decoded from the row the protocol's Finding gives a link, listed
/// under their heading with jawd's marked words, Enter opening one and ⌘G going to its message, as
/// ⌘G goes to a message found.

/// `find`'s answer with a chat and two links, as the protocol's Finding has a link's row, since
/// protocol/fixtures/find.result.json has none.
let linksFound = """
{"results":[
 {"kind":"chat","score":40,"text":[{"t":"mark","s":"Sintra"},{"t":"text","s":" crew"}],"field":"name",
  "row":{"id":"c1","v":1,"sort":"1","kind":"group","name":"Sintra crew"}},
 {"kind":"link","score":0,"text":[{"t":"mark","s":"Sintra"},{"t":"text","s":": the routes · climb.pt"}],"chat_name":"Lisbon Climbing",
  "row":{"id":"3EB0A1B2C3D4E5F6~0","kind":"link","message":"3EB0A1B2C3D4E5F6","chat":"120363012345678901@g.us","chat_name":"Lisbon Climbing",
         "from":{"id":"person:7","name":"Rui","colour":2},"mine":false,"at":1792145060000,
         "link":{"url":"https://climb.pt/sintra","host":"climb.pt","title":"Sintra: the routes","description":"Granite slabs above the sea"}}},
 {"kind":"link","score":0,"text":[{"t":"text","s":"https://www.ipma.pt/pt/otempo/"}],
  "row":{"id":"3EB0C431F2A9B8E7D6~1","kind":"link","message":"3EB0C431F2A9B8E7D6","chat":"351912345678@s.whatsapp.net",
         "chat_name":"Ana Ribeiro","mine":true,"at":1792145000000,"link":{"url":"https://www.ipma.pt/pt/otempo/","host":"ipma.pt"}}}
],"counts":{"chat":1,"link":2}}
"""

@Test func `a link find found decodes with its message, its chat and its host`() throws {
    let found = try JSONDecoder().decode(Found.self, from: Data(linksFound.utf8))
    #expect(found.results.map(\.kind) == [.chat, .link, .link] && found.counts[.link] == 2)
    guard case let .link(rui) = found.results[1].row, case let .link(yours) = found.results[2].row else {
        Issue.record("the links read as untyped rows")
        return
    }
    #expect(rui.id == "3EB0A1B2C3D4E5F6~0" && rui.message == "3EB0A1B2C3D4E5F6" && rui.chat == "120363012345678901@g.us")
    #expect(rui.chatName == "Lisbon Climbing" && rui.from?.name == "Rui" && !rui.mine && rui.at == 1_792_145_060_000)
    #expect(rui.link.host == "climb.pt" && rui.link.title == "Sintra: the routes" && rui.link.description == "Granite slabs above the sea")
    #expect(yours.mine && yours.from == nil && yours.link.title == nil && yours.link.url == "https://www.ipma.pt/pt/otempo/")
}

@MainActor @Test func `the palette lists the links found, Enter opens one, and ⌘G goes to its message`() throws {
    let model = PaletteModel()
    var chosen: [String] = []
    model.chose = { pick in
        switch pick {
        case let .link(l): chosen.append("open \(l.link.url)")
        case let .linkMessage(l): chosen.append("go to \(l.message) in \(l.chat)")
        default: chosen.append("something else")
        }
    }
    model.reset()
    model.answered(id: 1, try JSONDecoder().decode(Found.self, from: Data(linksFound.utf8)))
    let links = model.items.filter { $0.group == .kind(.link) }
    #expect(links.count == 2, "listed, where they were left out")
    #expect(links[0].title.map(\.s).joined() == "Sintra: the routes · climb.pt" && links[0].title.first?.t == .mark, "jawd's words, marked")
    #expect(links[0].detail.hasPrefix("Rui · Lisbon Climbing · ") && links[1].detail.hasPrefix("You · Ana Ribeiro · "))
    model.pick(links[0])
    model.index = try #require(model.items.firstIndex { $0.id == links[1].id })
    #expect(model.goToLink())
    model.pick(newPane: true)
    #expect(chosen == ["open https://climb.pt/sintra", "go to 3EB0C431F2A9B8E7D6 in 351912345678@s.whatsapp.net",
                       "open https://www.ipma.pt/pt/otempo/"], "⌘↩ on a link does what ↩ does")
    model.index = 0
    #expect(!model.goToLink(), "a chat isn't a link")
    #expect(PaletteModel.name(model.found.results[2]) == "https://www.ipma.pt/pt/otempo/", "one with no title goes by its URL")
    #expect(PaletteModel.name(model.found.results[1]) == "Sintra: the routes · climb.pt")
}
