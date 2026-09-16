extension NativeCLI {
    static let createWindowsHelp = """
    create-windows NAME --iso ABSOLUTE_PATH
      Create a saved installation-pending own-HVF Windows VM. The ISO is copied
      into its managed bundle before vm.json is published. This command does not
      install Windows, boot a guest, open a window, or accept passwords, recovery
      keys, unattended files, guest payloads, drivers, or experimental graphics.

      Optional product settings:
        --disk-gib 64|96|128|256|512
        --memory-mib 2048|4096|6144|8192|12288|16384|24576|32768
        --cpus COUNT
        --resolution 1280x800|1440x900|1920x1080|2560x1440
        --no-network

      Defaults: 64 GiB, 6144 MiB, 4 CPUs, 1440x900, shared NAT network.
      On success, use the returned exact ID with `bridgevm app install ID`.
      JSON schema: bridgevm.app-create-windows.v1.
      Exit codes: 0 registration confirmed, 1 creation/confirmation failed,
      2 invalid name, option, path, media, resource value or host CPU request.
    """
}
