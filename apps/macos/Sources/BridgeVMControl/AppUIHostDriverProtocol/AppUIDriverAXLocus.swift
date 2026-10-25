#if (DEBUG && BRIDGEVM_APP_UI_HOST) || BRIDGEVM_APP_UI_DRIVER
import Foundation

/// Where the AX call of a d6 request's read-only pass failed, recorded after that
/// pass failed: positions, counts, AX constants and attribute names only, never a
/// title, value, description, label, help text or identifier.
struct AppUIDriverAXLocus: Codable, Equatable {
    static let maximumPath = 32, maximumNames = 64, maximumVisited = 20_000, maximumDistinct = 256
    static let unreadable = "unreadable", absent = "absent"
    /// AXChildren steps below the main window. -1 is the window lookup, where
    /// `indexPath` is [] for the application or [i] for its i-th AXWindows entry.
    let depth: Int
    /// The first 32 AXChildren indices from the main window down.
    let indexPath: [Int]
    /// Elements whose children the pass had read before the failing call.
    let visited: Int
    let role: String, subrole: String, parentRole: String
    /// Sorted, unique AX constants from the element's attribute names; empty when unreadable.
    let attributeNames: [String]
    /// AXIdentifier is among all of those names, including any past the 64th.
    let identifierListed: Bool

    /// `^AX[A-Za-z]{1,40}$`
    static func isConstant(_ value: String) -> Bool {
        let bytes = Array(value.utf8)
        return (3...42).contains(bytes.count) && bytes[0] == 65 && bytes[1] == 88
            && bytes.dropFirst(2).allSatisfy { (65...90).contains($0) || (97...122).contains($0) }
    }

    static func isRoleField(_ value: String) -> Bool { isConstant(value) || value == unreadable || value == absent }

    /// The calls a read-only pass makes; a mutation, its preparation and the readback never carry a locus.
    static let passAPIs: Set<AppUIDriverAXContext.API> = [.getPID, .setMessagingTimeout, .copyAttributeValue,
                                                          .copyAttributeValues, .getAttributeValueCount]

    /// A locus comes only with its distinct count, only where the reply admits one, on a pass read.
    static func validate(_ context: AppUIDriverAXContext, allowed: Bool) throws {
        guard let locus = context.locus else {
            guard context.distinctLoci == nil else { throw AppUIDriverFailure.invalidReply }
            return
        }
        guard allowed, let distinct = context.distinctLoci, (1...maximumDistinct).contains(distinct),
              passAPIs.contains(context.api), context.attribute != .none, context.attribute != .other,
              locus.isWellFormed else { throw AppUIDriverFailure.invalidReply }
    }

    var isWellFormed: Bool {
        let names = attributeNames
        // The 64-name bound can drop AXIdentifier only when all 64 kept names sort before it.
        let cut = names.count == Self.maximumNames && names.last.map { $0 < "AXIdentifier" } == true
        let tree = depth >= 0 && indexPath.count == min(depth, Self.maximumPath) && depth <= visited
        let lookup = depth == -1 && visited == 0
            && (indexPath.count == 1 || (indexPath.isEmpty && parentRole == Self.absent))
        return (tree || lookup) && visited <= Self.maximumVisited
            && indexPath.allSatisfy { (0..<10_000).contains($0) }
            && [role, subrole, parentRole].allSatisfy(Self.isRoleField)
            && names.count <= Self.maximumNames && names.allSatisfy(Self.isConstant)
            && zip(names, names.dropFirst()).allSatisfy { $0 < $1 }
            && (names.contains("AXIdentifier") ? identifierListed : !identifierListed || cut)
    }
}
#endif
