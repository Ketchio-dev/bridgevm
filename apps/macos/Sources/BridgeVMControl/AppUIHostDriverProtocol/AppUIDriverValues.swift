#if (DEBUG && BRIDGEVM_APP_UI_HOST) || BRIDGEVM_APP_UI_DRIVER
import Foundation

struct AppUIDriverValues: Codable, Equatable {
    var createVisible: Bool?
    var importVisible: Bool?
    var commitVisible: Bool?
    var nameVisible: Bool?
    var amberVisible: Bool?
    var indigoVisible: Bool?
    var isoSelection: String?
    var payloadSelection: String?
    var manifestSelection: String?
    var searchValue: String?

    init(createVisible: Bool? = nil, importVisible: Bool? = nil,
         commitVisible: Bool? = nil, nameVisible: Bool? = nil,
         amberVisible: Bool? = nil, indigoVisible: Bool? = nil,
         isoSelection: String? = nil, payloadSelection: String? = nil,
         manifestSelection: String? = nil, searchValue: String? = nil) {
        self.createVisible = createVisible; self.importVisible = importVisible
        self.commitVisible = commitVisible; self.nameVisible = nameVisible
        self.amberVisible = amberVisible; self.indigoVisible = indigoVisible
        self.isoSelection = isoSelection; self.payloadSelection = payloadSelection
        self.manifestSelection = manifestSelection; self.searchValue = searchValue
    }

    var presentFields: Set<String> {
        var result = Set<String>()
        if createVisible != nil { result.insert("createVisible") }
        if importVisible != nil { result.insert("importVisible") }
        if commitVisible != nil { result.insert("commitVisible") }
        if nameVisible != nil { result.insert("nameVisible") }
        if amberVisible != nil { result.insert("amberVisible") }
        if indigoVisible != nil { result.insert("indigoVisible") }
        if isoSelection != nil { result.insert("isoSelection") }
        if payloadSelection != nil { result.insert("payloadSelection") }
        if manifestSelection != nil { result.insert("manifestSelection") }
        if searchValue != nil { result.insert("searchValue") }
        return result
    }
}
#endif
