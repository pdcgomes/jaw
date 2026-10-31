import AppKit
import Contacts
import ContactsUI
import JawUI

/// The system's contact picker, shown from the contact form's Add a Contact…: the contact picked is
/// made into its card off the main thread and put on the form, fetched again with what a vCard
/// needs when the picker gave less, which asks for Contacts the first time; when Contacts isn't
/// allowed, the form says where to allow it.
@MainActor
final class ContactPicking: NSObject, CNContactPickerDelegate {
    var picker: CNContactPicker?
    weak var form: ContactForm?

    nonisolated static let notAllowed = "Jaw isn't allowed to see your contacts: allow it in System Settings › Privacy & Security › Contacts"

    func pick(for form: ContactForm, under view: NSView) {
        picker?.close()
        let picker = CNContactPicker()
        picker.displayedKeys = [CNContactPhoneNumbersKey]
        picker.delegate = self
        self.picker = picker
        self.form = form
        picker.showRelative(to: view.bounds, of: view, preferredEdge: view.isFlipped ? .maxY : .minY)
    }

    func close() {
        picker?.close()
        picker = nil
        form = nil
    }

    /// A contact the picker gave, handed from thread to thread: Contacts documents a CNContact as
    /// immutable and safe to read from any thread.
    struct Picked: @unchecked Sendable {
        let contact: CNContact
    }

    nonisolated func contactPicker(_: CNContactPicker, didSelect contact: CNContact) {
        let given = Picked(contact: contact)
        MainActor.assumeIsolated { picked(given) }
    }

    nonisolated func contactPicker(_: CNContactPicker, didSelect contactProperty: CNContactProperty) {
        let given = Picked(contact: contactProperty.contact)
        MainActor.assumeIsolated { picked(given) }
    }

    func picked(_ given: Picked) {
        picker?.close()
        guard let form else { return }
        Task.detached(priority: .userInitiated) {
            let made = await Self.card(of: given.contact)
            await MainActor.run { form.land(made) }
        }
    }

    /// `contact`'s card, from the contact the picker gave, or from Contacts, asked for everything
    /// the card is made from, when it gave less.
    nonisolated static func card(of contact: CNContact) async -> ContactForm.Made {
        let made = ContactForm.made(contact)
        guard made == .unfetched else { return made }
        let store = CNContactStore()
        if CNContactStore.authorizationStatus(for: .contacts) == .notDetermined {
            _ = try? await store.requestAccess(for: .contacts)
        }
        guard CNContactStore.authorizationStatus(for: .contacts) == .authorized else { return .unadded(notAllowed) }
        do {
            return try ContactForm.made(store.unifiedContact(withIdentifier: contact.identifier, keysToFetch: ContactForm.keys))
        } catch {
            return .unadded("Contacts didn't give Jaw that contact: \(error.localizedDescription)")
        }
    }
}
