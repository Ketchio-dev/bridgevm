import SwiftUI

/// One bridge span between two abutments; no overlapping window outlines.
struct BridgeVMMark: Shape {
    func path(in rect: CGRect) -> Path { Path(Self.cgPath(in: rect)) }

    static func cgPath(in rect: CGRect) -> CGPath {
        let scale = min(rect.width, rect.height) / 64
        let origin = CGPoint(x: rect.midX - 32 * scale, y: rect.midY - 32 * scale)
        func point(_ x: CGFloat, _ y: CGFloat) -> CGPoint {
            CGPoint(x: origin.x + x * scale, y: origin.y + y * scale)
        }
        let path = CGMutablePath()
        path.move(to: point(6, 50))
        path.addLine(to: point(6, 18))
        path.addQuadCurve(to: point(10, 14), control: point(6, 14))
        path.addLine(to: point(17, 14))
        path.addQuadCurve(to: point(21, 18), control: point(21, 14))
        path.addLine(to: point(21, 27))
        path.addCurve(to: point(43, 27), control1: point(27, 19), control2: point(37, 19))
        path.addLine(to: point(43, 18))
        path.addQuadCurve(to: point(47, 14), control: point(43, 14))
        path.addLine(to: point(54, 14))
        path.addQuadCurve(to: point(58, 18), control: point(58, 14))
        path.addLine(to: point(58, 50))
        path.addQuadCurve(to: point(54, 54), control: point(58, 54))
        path.addLine(to: point(47, 54))
        path.addQuadCurve(to: point(43, 50), control: point(43, 54))
        path.addLine(to: point(43, 43))
        path.addCurve(to: point(21, 43), control1: point(37, 35), control2: point(27, 35))
        path.addLine(to: point(21, 50))
        path.addQuadCurve(to: point(17, 54), control: point(21, 54))
        path.addLine(to: point(10, 54))
        path.addQuadCurve(to: point(6, 50), control: point(6, 54))
        path.closeSubpath()
        return path
    }
}
