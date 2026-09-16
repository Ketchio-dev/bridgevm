#if (DEBUG && BRIDGEVM_APP_UI_HOST) || BRIDGEVM_APP_UI_DRIVER
import Foundation

enum AppUIDriverOperationPolicy {
    static func operations(for phase: AppUIDriverPhase) -> Set<AppUIDriverOperation> {
        switch phase {
        case .welcome: return [.welcomeControls]
        case .creation: return [.pressCreate, .createForm]
        case .cancellation: return [.cancelCreate]
        case .importing: return [.pressImport, .importForm]
        case .overview: return [.pressOverview, .overviewCards]
        case .filtering: return [.setSearchIndigo, .searchState]
        case .clearing: return [.clearSearch, .searchState]
        }
    }

    static func mutation(for phase: AppUIDriverPhase) -> AppUIDriverOperation? {
        switch phase {
        case .welcome: return nil
        case .creation: return .pressCreate
        case .cancellation: return .cancelCreate
        case .importing: return .pressImport
        case .overview: return .pressOverview
        case .filtering: return .setSearchIndigo
        case .clearing: return .clearSearch
        }
    }

    static func isMutation(_ operation: AppUIDriverOperation) -> Bool {
        [.pressCreate, .cancelCreate, .pressImport, .pressOverview, .setSearchIndigo, .clearSearch].contains(operation)
    }

    static func window(for operation: AppUIDriverOperation) -> AppUIDriverWindow {
        [.createForm, .cancelCreate].contains(operation) ? .creationSheet : .main
    }

    static func fields(for operation: AppUIDriverOperation) -> Set<String> {
        switch operation {
        case .welcomeControls: return ["createVisible", "importVisible"]
        case .createForm: return ["commitVisible", "isoSelection", "payloadSelection", "manifestSelection"]
        case .importForm: return ["nameVisible"]
        case .overviewCards: return ["amberVisible", "indigoVisible"]
        case .searchState: return ["amberVisible", "indigoVisible", "searchValue"]
        case .setSearchIndigo: return ["searchValue"]
        default: return []
        }
    }

    static func completes(_ phase: AppUIDriverPhase, reply: AppUIDriverReply) -> Bool {
        guard reply.outcome != .refused else { return false }
        let values = reply.values
        switch phase {
        case .welcome: return values?.createVisible == true && values?.importVisible == true
        case .creation:
            return values?.commitVisible == true && values?.isoSelection == ""
                && values?.payloadSelection == "" && values?.manifestSelection == ""
        case .cancellation: return reply.operation == .cancelCreate && reply.outcome == .performed
        case .importing: return values?.nameVisible == true
        case .overview: return values?.amberVisible == true && values?.indigoVisible == true
        case .filtering:
            return reply.operation == .searchState && values?.amberVisible == false
                && values?.indigoVisible == true && values?.searchValue == "Indigo"
        case .clearing:
            return values?.amberVisible == true && values?.indigoVisible == true && values?.searchValue == ""
        }
    }
}
#endif
