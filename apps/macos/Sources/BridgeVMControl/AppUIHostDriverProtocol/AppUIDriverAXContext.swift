#if (DEBUG && BRIDGEVM_APP_UI_HOST) || BRIDGEVM_APP_UI_DRIVER
import Foundation

// Diagnostic vocabulary only: never serialize an AX value or an arbitrary attribute name.
struct AppUIDriverAXContext: Codable, Equatable {
    enum API: String, Codable, CaseIterable {
        case getPID, setMessagingTimeout, copyAttributeValue, copyAttributeValues, getAttributeValueCount
        case isAttributeSettable, performAction, setAttributeValue
    }
    enum Scope: String, Codable { case element, relatedElement }
    let api: API
    let attribute: Attribute
    let scope: Scope
    /// Only a d6 refusal raised inside the read-only pass has a locus (see AppUIDriverAXLocus).
    var locus: AppUIDriverAXLocus? = nil
    /// The distinct index paths that failed over that request's passes, with the locus only.
    var distinctLoci: Int? = nil

    /// `locus` admits a locus; only the d6 reply does, as the install-cancel driver never records one.
    func validate(outcome: AppUIDriverOutcome, failure: AppUIDriverFailure?, rawValue: Int32?, locus: Bool = false) throws {
        guard outcome == .refused, failure == .axFailure, let rawValue, rawValue != 0,
              scope != .relatedElement || api == .getPID || api == .setMessagingTimeout else {
            throw AppUIDriverFailure.invalidReply
        }
        try AppUIDriverAXLocus.validate(self, allowed: locus)
    }
}
#endif
