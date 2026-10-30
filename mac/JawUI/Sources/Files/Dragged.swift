import AppKit
import UniformTypeIdentifiers

/// A file dragged out before it's here: a promise the Finder and Mail take, kept by `save`, which
/// has jawd download it and copy it whole into the folder it's dropped in, under the name the drop
/// gives it, or a name with a number when that's taken, since jawd never writes over a file. Once
/// it's here a file drags out as its clone instead, which any app takes.
public final class Dragged: NSFilePromiseProvider, NSFilePromiseProviderDelegate, @unchecked Sendable {
    public typealias Save = @MainActor @Sendable (URL, @escaping @Sendable ((any Error)?) -> Void) -> Void

    let name: String
    let save: Save
    static let writing: OperationQueue = {
        let queue = OperationQueue()
        queue.qualityOfService = .userInitiated
        return queue
    }()

    public init(_ offer: MediaOffer, save: @escaping Save) {
        name = offer.name
        self.save = save
        super.init()
        fileType = UTType(filenameExtension: (offer.name as NSString).pathExtension)?.identifier ?? UTType.data.identifier
        delegate = self
    }

    public func filePromiseProvider(_: NSFilePromiseProvider, fileNameForType _: String) -> String {
        name
    }

    public func filePromiseProvider(_: NSFilePromiseProvider, writePromiseTo url: URL, completionHandler: @escaping ((any Error)?) -> Void) {
        // The promise is kept once, from whatever thread jawd's answer is taken on.
        nonisolated(unsafe) let handler = completionHandler
        let done: @Sendable ((any Error)?) -> Void = { handler($0) }
        DispatchQueue.main.async { [save] in MainActor.assumeIsolated { save(url, done) } }
    }

    public func operationQueue(for _: NSFilePromiseProvider) -> OperationQueue {
        Self.writing
    }
}

/// Why a promised file wasn't written: jawd's words.
public struct NotSaved: LocalizedError, Sendable {
    public let why: String

    public init(_ why: String) {
        self.why = why
    }

    public var errorDescription: String? {
        why
    }
}
