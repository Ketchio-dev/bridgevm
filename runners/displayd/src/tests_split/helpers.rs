//! Split test module.

use crate::*;

pub(super) fn cli(visibility: Visibility, dirty_regions: u16) -> Cli {
    Cli {
        print_plan: true,
        visibility,
        dirty_regions,
        framebuffer_width: 1440,
        framebuffer_height: 900,
        scale: 2,
        cursor_overlay: true,
        resize_width: None,
        resize_height: None,
        cursor_x: None,
        cursor_y: None,
        sample_frames: 0,
        frame_time_micros: 0,
        frame_sample_file: None,
        framebuffer_rgba_file: None,
        window_crop_rgba_file: None,
        runtime_policy_file: None,
        window_id: None,
        window_title: None,
        window_x: None,
        window_y: None,
        window_width: None,
        window_height: None,
        window_host_width: None,
        window_host_height: None,
    }
}
