//! Legacy create-request construction.

use crate::*;

pub(crate) fn request_for_create(args: CreateArgs) -> Result<BridgeVmRequest> {
    if create_args_are_plain_template_request(&args) {
        return Ok(BridgeVmRequest::CreateVmFromTemplate {
            name: args.name,
            template_id: args.template.expect("plain template request has template"),
        });
    }

    let manifest = manifest_for_create(args)?;
    Ok(BridgeVmRequest::create_vm(manifest))
}

pub(crate) fn create_args_are_plain_template_request(args: &CreateArgs) -> bool {
    args.template.is_some()
        && args.os.is_none()
        && args.version.is_none()
        && args.arch.is_none()
        && args.mode == ModeChoice::Auto
        && args.disk.is_none()
        && args.disk_format.is_none()
        && args.boot_mode.is_none()
        && args.installer_image.is_none()
        && args.kernel_path.is_none()
        && args.initrd_path.is_none()
        && args.kernel_command_line.is_none()
        && args.macos_restore_image.is_none()
}
