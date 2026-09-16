#if DEBUG && BRIDGEVM_APP_UI_HOST
import AppKit

@MainActor
enum AppUIHostGeometryObservation {
    static func snapshot(_ window: NSWindow, content: NSView, minimum: Bool) -> [String: Any] {
        let convertedContent = window.contentRect(forFrameRect: window.frame)
        let requestedContent = NSRect(origin: convertedContent.origin,
            size: NSSize(width: minimum ? 1100 : 1320, height: minimum ? 720 : 860))
        let requestedFrame = window.frameRect(forContentRect: requestedContent)
        return [
            "nonfinite_numbers_encoded_as_null": true,
            "window_min_frame_size_points": size(window.minSize),
            "window_max_frame_size_points": size(window.maxSize),
            "window_min_content_size_window_points": size(window.contentMinSize),
            "window_max_content_size_window_points": size(window.contentMaxSize),
            "window_resize_increments_frame_points": size(window.resizeIncrements),
            "window_content_resize_increments_window_points": size(window.contentResizeIncrements),
            "window_aspect_ratio": size(window.aspectRatio),
            "window_content_aspect_ratio": size(window.contentAspectRatio),
            "original_content_fitting_size_local_points": size(content.fittingSize),
            "original_content_intrinsic_size_local_points": size(content.intrinsicContentSize),
            "original_content_translates_autoresizing_mask": content.translatesAutoresizingMaskIntoConstraints,
            "current_frame_to_content_rect_screen_points": finiteRect(convertedContent),
            "requested_content_rect_screen_points": finiteRect(requestedContent),
            "requested_content_to_frame_rect_screen_points": finiteRect(requestedFrame),
            "original_content_horizontal_constraints": constraints(
                content.constraintsAffectingLayout(for: .horizontal), content: content),
            "original_content_vertical_constraints": constraints(
                content.constraintsAffectingLayout(for: .vertical), content: content)
        ]
    }

    static func presentationRect(_ rect: NSRect) -> [String: Double] {
        ["x": Double(rect.origin.x), "y": Double(rect.origin.y),
         "width": Double(rect.width), "height": Double(rect.height)]
    }

    private static func constraints(_ values: [NSLayoutConstraint], content: NSView) -> [String: Any] {
        let records: [[String: Any]] = values.prefix(16).map { constraint in
            ["first_attribute": constraint.firstAttribute.rawValue,
             "second_attribute": constraint.secondAttribute.rawValue,
             "relation": constraint.relation.rawValue,
             "multiplier": finite(Double(constraint.multiplier)),
             "constant": finite(Double(constraint.constant)),
             "priority": finite(Double(constraint.priority.rawValue)),
             "active": constraint.isActive,
             "first_item_is_original_content": (constraint.firstItem as AnyObject?) === content,
             "second_item_is_original_content": (constraint.secondItem as AnyObject?) === content]
        }
        return ["total_count": values.count, "recorded_count": records.count,
                "truncated": values.count > records.count, "records": records]
    }

    private static func size(_ value: NSSize) -> [String: Any] {
        ["width": finite(Double(value.width)), "height": finite(Double(value.height))]
    }

    private static func finiteRect(_ value: NSRect) -> [String: Any] {
        presentationRect(value).mapValues { finite($0) }
    }

    private static func finite(_ value: Double) -> Any {
        value.isFinite ? value as Any : NSNull()
    }
}
#endif
