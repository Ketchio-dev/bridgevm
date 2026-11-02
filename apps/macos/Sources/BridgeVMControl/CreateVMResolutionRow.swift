import SwiftUI

struct CreateVMResolutionRow: View {
    let usesGuestScreen: Bool
    @Binding var selection: Int
    let resolutions: [(Int, Int)]

    var body: some View {
        HStack {
            Text("해상도").frame(width: 64, alignment: .leading)
            if usesGuestScreen {
                Text("게스트 화면 크기를 사용합니다.")
                    .font(.callout).foregroundColor(.secondary)
            } else {
                Picker("해상도", selection: $selection) {
                    ForEach(0..<resolutions.count, id: \.self) { i in
                        Text("\(resolutions[i].0)×\(resolutions[i].1)").tag(i)
                    }
                }.labelsHidden().frame(width: 150)
            }
            Spacer()
        }
    }
}
