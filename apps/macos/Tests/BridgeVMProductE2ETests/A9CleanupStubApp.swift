import Foundation
@testable import BridgeVMProductE2E

/// Stands in for the product's `--vtpm-lifecycle forget-import`. It records its arguments, keeps one file per key
/// under `keychain/`, prints the product's proof line when it deletes a key and the product's missing-key refusal
/// when there is none.
enum A9CleanupStubApp {
    static func script(capture: URL, keychain: URL) -> String {
        """
        #!/bin/sh
        printf '%s ' "$@" >> '\(capture.path)'; printf '\\n' >> '\(capture.path)'
        id=''; while [ $# -gt 0 ]; do [ "$1" = --stable-vm-id ] && id="$2"; shift; done
        if [ -n "$id" ] && [ -f '\(keychain.path)'/"$id" ]; then
          rm '\(keychain.path)'/"$id"; echo "forgot_imported_key=$id"; exit 0
        fi
        echo '\(A9KeyForgetCall.missingKeyLine)' >&2; exit 1

        """
    }
}
