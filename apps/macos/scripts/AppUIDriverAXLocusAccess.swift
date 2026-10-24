import ApplicationServices
import Foundation

/// Records where a read of one d6 request's read-only AX pass failed, for its refusal.
///
/// A pass starts at the application element, where each pass's window lookup
/// starts, and ends at the first mutation, so a press, a search value and the
/// readback after it are outside it. Each element the pass reaches has a place:
/// the application and its AXWindows entries are the window lookup (depth -1);
/// the first element whose children the pass reads is the main window (depth 0,
/// index path []), and each AXChildren entry is one step below its parent, by its
/// index in that parent's children. When a read inside the pass throws an AX
/// error, and only then, the element it was made on is described by that place,
/// its AXRole and AXSubrole, its parent's AXRole and its attribute names. Those
/// reads follow the failure, never throw out of the refusal and never mutate; no
/// title, value, description, label, help text or identifier is read. One instance
/// serves one request, so its distinct count spans that request's restarts.
final class AppUIDriverAXLocusAccess<Base: AppUIDriverAXAccess>: AppUIDriverAXAccess {
    typealias Element = Base.Element
    typealias L = AppUIDriverAXLocus
    private final class Place {
        let element: Element, depth: Int, path: [Int], parent: Element?
        var expanded = false
        init(_ element: Element, depth: Int, path: [Int], parent: Element?) {
            self.element = element; self.depth = depth; self.path = path; self.parent = parent
        }
    }
    let base: Base
    private let names: (Element) throws -> [String]
    private var active = false, app: Element?, visited = 0
    private var lookup: [UInt: [Place]] = [:], tree: [UInt: [Place]] = [:]
    private var distinct: Set<[Int]> = []

    init(_ base: Base, names: @escaping (Element) throws -> [String]) {
        self.base = base; self.names = names
    }

    func application() -> Element {
        let node = base.application()
        active = true; app = node; visited = 0; lookup = [:]; tree = [:]
        insert(Place(node, depth: -1, path: [], parent: nil), into: &lookup)
        return node
    }

    func related(_ node: Element, _ attribute: String) throws -> [Element] {
        guard active else { return try base.related(node, attribute) }
        let expanding = attribute == AppUIDriverAXAttribute.children
        if expanding && find(node, in: tree) == nil {
            insert(Place(node, depth: 0, path: [], parent: find(node, in: lookup)?.parent), into: &tree)
        }
        let result: [Element]
        do { result = try base.related(node, attribute) } catch { throw located(error, node) }
        if expanding, let parent = find(node, in: tree) {
            if !parent.expanded { parent.expanded = true; visited += 1 }
            for (index, child) in result.enumerated() where find(child, in: tree) == nil {
                let path = parent.path.count < L.maximumPath ? parent.path + [index] : parent.path
                insert(Place(child, depth: parent.depth + 1, path: path, parent: node), into: &tree)
            }
        } else if attribute == AppUIDriverAXAttribute.windows, let app, base.same(node, app) {
            for (index, window) in result.enumerated() where find(window, in: lookup) == nil {
                insert(Place(window, depth: -1, path: [index], parent: app), into: &lookup)
            }
        }
        return result
    }

    func string(_ node: Element, _ attribute: String) throws -> String? {
        do { return try base.string(node, attribute) } catch { throw active ? located(error, node) : error }
    }
    func enabled(_ node: Element) throws -> Bool {
        do { return try base.enabled(node) } catch { throw active ? located(error, node) : error }
    }
    func press(_ node: Element) throws { active = false; try base.press(node) }
    func setSearch(_ node: Element) throws { active = false; try base.setSearch(node) }
    func hash(_ node: Element) -> UInt { base.hash(node) }
    func same(_ lhs: Element, _ rhs: Element) -> Bool { base.same(lhs, rhs) }

    private func find(_ node: Element, in table: [UInt: [Place]]) -> Place? {
        table[base.hash(node)]?.first { base.same($0.element, node) }
    }
    private func insert(_ place: Place, into table: inout [UInt: [Place]]) {
        table[base.hash(place.element), default: []].append(place)
    }

    /// The AX error with the element's locus; any other error, or one without a call context, is unchanged.
    private func located(_ error: Error, _ node: Element) -> Error {
        guard var failure = error as? AppUIDriverAXError, failure.failure == .axFailure, var context = failure.context,
              let place = find(node, in: tree) ?? find(node, in: lookup) else { return error }
        let role = constant(node, AppUIDriverAXAttribute.role), subrole = constant(node, "AXSubrole")
        let parentRole = place.parent.map { constant($0, AppUIDriverAXAttribute.role) } ?? L.absent
        let all = (try? names(node)) ?? []
        if distinct.count < L.maximumDistinct { distinct.insert([place.depth] + place.path) }
        context.locus = L(depth: place.depth, indexPath: place.path, visited: visited, role: role, subrole: subrole,
                          parentRole: parentRole, attributeNames: Array(Set(all.filter(L.isConstant)).sorted().prefix(L.maximumNames)),
                          identifierListed: all.contains(AppUIDriverAXAttribute.identifier))
        context.distinctLoci = distinct.count
        failure.context = context
        return failure
    }

    /// An AX constant; `absent` without a value; `unreadable` for a failed read or any other text.
    private func constant(_ node: Element, _ attribute: String) -> String {
        do {
            guard let value = try base.string(node, attribute) else { return L.absent }
            return L.isConstant(value) ? value : L.unreadable
        } catch { return L.unreadable }
    }
}

/// The attribute-name message, injectable like every AppUIDriverAXCalls closure; only a locus sends it.
struct AppUIDriverAXNameCalls {
    var copy: (AXUIElement, UnsafeMutablePointer<CFArray?>) -> AXError = { AXUIElementCopyAttributeNames($0, $1) }
}

/// The production wrapper's ownership, deadline and cancellation checks, which AppUIDriverAX declares.
protocol AppUIDriverAXPreparation {
    func prepare(_ node: AXUIElement, attribute: String?, scope: AppUIDriverAXContext.Scope) throws
    func check() throws
}

extension AppUIDriverAXPreparation {
    /// Every name the element lists, prepared like any other message: at most 1,024 names of at most 1,024 units.
    func attributeNames(_ node: AXUIElement, calls: AppUIDriverAXNameCalls) throws -> [String] {
        try prepare(node, attribute: nil, scope: .element)
        var value: CFArray?
        let result = calls.copy(node, &value)
        guard result == .success else { throw AppUIDriverAXError(failure: .axFailure, rawValue: result.rawValue) }
        try check()
        guard let value, CFGetTypeID(value) == CFArrayGetTypeID() else { throw AppUIDriverFailure.invalidElement }
        let count = CFArrayGetCount(value)
        guard count <= 1_024 else { throw AppUIDriverFailure.protocolLimit }
        return try (0..<count).map { index in
            guard let pointer = CFArrayGetValueAtIndex(value, index) else { throw AppUIDriverFailure.invalidElement }
            let item = unsafeBitCast(pointer, to: CFTypeRef.self)
            guard CFGetTypeID(item) == CFStringGetTypeID() else { throw AppUIDriverFailure.invalidElement }
            let name = unsafeBitCast(item, to: CFString.self)
            guard CFStringGetLength(name) <= 1_024 else { throw AppUIDriverFailure.protocolLimit }
            return name as String
        }
    }
}

extension AppUIDriverAXLocusAccess where Base: AppUIDriverAXPreparation, Base.Element == AXUIElement {
    /// The service's pass over the production wrapper, with the actual attribute-name message.
    convenience init(_ base: Base, calls: AppUIDriverAXNameCalls = AppUIDriverAXNameCalls()) {
        self.init(base, names: { try base.attributeNames($0, calls: calls) })
    }
}
