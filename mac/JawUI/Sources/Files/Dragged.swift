import AppKit
import UniformTypeIdentifiers

/// A file dragged out before it's here: a promise the Finder and Mail take, kept in the app. `save`
/// has jawd download the file and save it whole into a folder of the app's own, then the app puts it
/// where it was dropped, quarantined as a message's attachment, under the name the drop gives it, or
/// that name with a number when it's taken, never over a file; so what's written there is the app's
/// own, and `placed` says the name it took. Once it's here a file drags out as its clone instead,
/// which any app takes.
public final class Dragged: NSFilePromiseProvider, NSFilePromiseProviderDelegate, @unchecked Sendable {
    /// Has jawd save the file into a folder, answering with the file it wrote there, or why not.
    public typealias Save = @MainActor @Sendable (URL, @escaping @Sendable (Result<URL, any Error>) -> Void) -> Void
    public typealias Placed = @MainActor @Sendable (URL) -> Void

    let name: String
    /// The run's folder, where jawd saves it first.
    let folder: URL
    let save: Save
    let placed: Placed?
    static let writing: OperationQueue = {
        let queue = OperationQueue()
        queue.qualityOfService = .userInitiated
        return queue
    }()

    public init(_ offer: MediaOffer, in folder: URL = Opened.folder, save: @escaping Save, placed: Placed? = nil) {
        name = offer.name
        self.folder = folder
        self.save = save
        self.placed = placed
        super.init()
        fileType = UTType(filenameExtension: (offer.name as NSString).pathExtension)?.identifier ?? UTType.data.identifier
        delegate = self
    }

    public func filePromiseProvider(_: NSFilePromiseProvider, fileNameForType _: String) -> String {
        name
    }

    public func filePromiseProvider(_: NSFilePromiseProvider, writePromiseTo url: URL, completionHandler: @escaping ((any Error)?) -> Void) {
        // The promise is kept once, from the queue its file is put on.
        nonisolated(unsafe) let handler = completionHandler
        let scratch: URL
        do {
            scratch = try Opened.promised(in: folder)
        } catch {
            return handler(error)
        }
        let saved: @Sendable (Result<URL, any Error>) -> Void = { [placed] result in
            Self.writing.addOperation {
                defer { try? FileManager.default.removeItem(at: scratch) }
                let at: URL
                do {
                    at = try Opened.put(result.get(), at: url)
                } catch {
                    return handler(error)
                }
                try? Opened.handOver(at, readOnly: false)
                handler(nil)
                if let placed { DispatchQueue.main.async { MainActor.assumeIsolated { placed(at) } } }
            }
        }
        DispatchQueue.main.async { [save] in MainActor.assumeIsolated { save(scratch, saved) } }
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
