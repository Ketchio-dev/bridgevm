import ApplicationServices

extension AppUIDriverAXCallsFixture {
    func relationshipCalls() -> AppUIDriverAXArrayCalls {
        AppUIDriverAXArrayCalls(
            count: { _, name, output in
                self.attributes.append(name as String)
                output.pointee = self.reportedCount ?? self.array.count
                return self.result(.getAttributeValueCount)
            },
            read: { _, name, index, maximum, output in
                self.attributes.append(name as String); self.ranges.append((index, maximum))
                output.pointee = self.array as CFArray
                return self.result(.copyAttributeValues)
            })
    }
}
