import SwiftUI

struct CreateVMResolutionRow: View {
    let usesGuestScreen: Bool
    @Binding var selection: Int
    let resolutions: [(Int, Int)]

    var body: some View {
        HStack {
            Text("Resolution").frame(width: 64, alignment: .leading)
            if usesGuestScreen {
                Text("Uses the Windows display size.")
                    .font(.callout).foregroundColor(.secondary)
            } else {
                Picker("Resolution", selection: $selection) {
                    ForEach(0..<resolutions.count, id: \.self) { i in
                        Text("\(resolutions[i].0)×\(resolutions[i].1)").tag(i)
                    }
                }.labelsHidden().frame(width: 150)
            }
            Spacer()
        }
    }
}
