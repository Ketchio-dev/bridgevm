#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

usage() {
  cat <<'USAGE'
usage: scripts/prepare-vz-ubuntu-arm64-boot-artifacts.sh [options]

Prepare Apple Virtualization.framework Ubuntu Arm64 linux-kernel/raw artifacts
from an existing Ubuntu disk image. qemu-img is used only for offline image
inspection/conversion, never as the VM runtime.

The output directory is compatible with scripts/stage-vz-ubuntu-desktop-vm.sh:
  <output-dir>/root.raw
  <output-dir>/vmlinuz
  <output-dir>/initrd
  <output-dir>/artifacts.json

Options:
  --source-image PATH    Source Ubuntu Arm64 disk image
                         (default: target/live-images/noble-server-cloudimg-arm64.img)
  --output-dir DIR       Output artifact directory
                         (default: target/vz-ubuntu-arm64-artifacts/noble)
  --disk-size SIZE       Optional qemu-img resize target for root.raw
  --prep-backend NAME    auto or docker-offline (default: auto)
  --docker-image IMAGE   Docker image for docker-offline extraction
                         (default: ubuntu:24.04)
  --allow-docker-pull    Allow Docker to pull --docker-image if missing
  --stage-name NAME      Also stage a BridgeVM bundle with this VM name
  --store DIR            BridgeVM store for --stage-name
                         (default: BRIDGEVM_HOME or ~/.bridgevm)
  --force                Replace existing output artifacts
  --dry-run              Print planned work and exports without writing
  -h, --help             Show this help

Environment overrides:
  BRIDGEVM_UBUNTU_BOOT_ARTIFACT_SOURCE_IMAGE
  BRIDGEVM_UBUNTU_BOOT_ARTIFACT_OUTPUT_DIR
  BRIDGEVM_UBUNTU_BOOT_ARTIFACT_DISK_SIZE
  BRIDGEVM_UBUNTU_BOOT_ARTIFACT_PREP_BACKEND
  BRIDGEVM_UBUNTU_BOOT_ARTIFACT_DOCKER_IMAGE
  BRIDGEVM_UBUNTU_BOOT_ARTIFACT_ALLOW_DOCKER_PULL

Notes:
  - AppleVzRunner currently launches raw disks only for this Linux path.
  - The generated kernel command line prefers root=UUID=<rootfs-uuid>.
  - systemd.unit=graphical.target is appended only if a desktop stack is found.
USAGE
}

SOURCE_IMAGE="${BRIDGEVM_UBUNTU_BOOT_ARTIFACT_SOURCE_IMAGE:-target/live-images/noble-server-cloudimg-arm64.img}"
OUTPUT_DIR="${BRIDGEVM_UBUNTU_BOOT_ARTIFACT_OUTPUT_DIR:-target/vz-ubuntu-arm64-artifacts/noble}"
DISK_SIZE="${BRIDGEVM_UBUNTU_BOOT_ARTIFACT_DISK_SIZE:-}"
PREP_BACKEND="${BRIDGEVM_UBUNTU_BOOT_ARTIFACT_PREP_BACKEND:-auto}"
DOCKER_IMAGE="${BRIDGEVM_UBUNTU_BOOT_ARTIFACT_DOCKER_IMAGE:-ubuntu:24.04}"
ALLOW_DOCKER_PULL="${BRIDGEVM_UBUNTU_BOOT_ARTIFACT_ALLOW_DOCKER_PULL:-0}"
STORE="${BRIDGEVM_HOME:-${HOME:-.}/.bridgevm}"
STAGE_NAME=""
FORCE=0
DRY_RUN=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --source-image)
      SOURCE_IMAGE="${2:?missing value for --source-image}"
      shift 2
      ;;
    --output-dir)
      OUTPUT_DIR="${2:?missing value for --output-dir}"
      shift 2
      ;;
    --disk-size)
      DISK_SIZE="${2:?missing value for --disk-size}"
      shift 2
      ;;
    --prep-backend)
      PREP_BACKEND="${2:?missing value for --prep-backend}"
      shift 2
      ;;
    --docker-image)
      DOCKER_IMAGE="${2:?missing value for --docker-image}"
      shift 2
      ;;
    --allow-docker-pull)
      ALLOW_DOCKER_PULL=1
      shift
      ;;
    --stage-name)
      STAGE_NAME="${2:?missing value for --stage-name}"
      shift 2
      ;;
    --store)
      STORE="${2:?missing value for --store}"
      shift 2
      ;;
    --force)
      FORCE=1
      shift
      ;;
    --dry-run)
      DRY_RUN=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

case "$PREP_BACKEND" in
  auto|docker-offline) ;;
  *)
    echo "error: --prep-backend must be auto or docker-offline" >&2
    exit 2
    ;;
esac

abs_path() {
  case "$1" in
    /*) printf '%s\n' "$1" ;;
    *) printf '%s\n' "$ROOT/$1" ;;
  esac
}

SOURCE_IMAGE_ABS="$(abs_path "$SOURCE_IMAGE")"
OUTPUT_DIR_ABS="$(abs_path "$OUTPUT_DIR")"
RAW_DISK="$OUTPUT_DIR_ABS/root.raw"
KERNEL="$OUTPUT_DIR_ABS/vmlinuz"
INITRD="$OUTPUT_DIR_ABS/initrd"
EXTRACTION_JSON="$OUTPUT_DIR_ABS/extraction.json"
ARTIFACTS_JSON="$OUTPUT_DIR_ABS/artifacts.json"

require_tool() {
  local tool="$1"
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "error: required tool is missing: $tool" >&2
    exit 1
  fi
}

have_tool() {
  command -v "$1" >/dev/null 2>&1
}

file_sha256() {
  local path="$1"
  if have_tool shasum; then
    shasum -a 256 "$path" | awk '{print $1}'
  else
    sha256sum "$path" | awk '{print $1}'
  fi
}

file_size_bytes() {
  wc -c <"$1" | tr -d ' '
}

normalize_kernel_for_apple_vz() {
  local magic
  magic="$(od -An -tx1 -N2 "$KERNEL" | tr -d ' \n')"
  if [[ "$magic" == "1f8b" ]]; then
    require_tool gzip
    local tmp
    tmp="$(mktemp "$KERNEL.uncompressed.XXXXXX")"
    gzip -dc "$KERNEL" >"$tmp"
    mv "$tmp" "$KERNEL"
    printf '%s\n' "gzip-decompressed"
  else
    printf '%s\n' "unchanged"
  fi
}

select_backend() {
  if [[ "$PREP_BACKEND" != "auto" ]]; then
    printf '%s\n' "$PREP_BACKEND"
    return
  fi
  if have_tool docker; then
    echo "docker-offline"
  else
    echo "none"
  fi
}

print_exports() {
  local kernel_cmdline="${1:-}"
  printf 'export BRIDGEVM_UBUNTU_VZ_KERNEL=%q\n' "$KERNEL"
  printf 'export BRIDGEVM_UBUNTU_VZ_INITRD=%q\n' "$INITRD"
  printf 'export BRIDGEVM_UBUNTU_VZ_RAW_DISK=%q\n' "$RAW_DISK"
  printf 'export BRIDGEVM_UBUNTU_VZ_ARTIFACTS_JSON=%q\n' "$ARTIFACTS_JSON"
  if [[ -n "$kernel_cmdline" ]]; then
    printf 'export BRIDGEVM_UBUNTU_VZ_KERNEL_CMDLINE=%q\n' "$kernel_cmdline"
    printf 'export BRIDGEVM_LIVE_VZ_KERNEL_CMDLINE=%q\n' "$kernel_cmdline"
  fi
  printf 'export BRIDGEVM_LIVE_VZ_KERNEL=%q\n' "$KERNEL"
  printf 'export BRIDGEVM_LIVE_VZ_INITRD=%q\n' "$INITRD"
  printf 'export BRIDGEVM_LIVE_VZ_RAW_DISK=%q\n' "$RAW_DISK"
  printf 'export BRIDGEVM_LIVE_VZ_ARTIFACTS_JSON=%q\n' "$ARTIFACTS_JSON"
  printf 'export BRIDGEVM_LIVE_VZ_SERIAL_EXPECTED=%q\n' "Ubuntu"
}

docker_extract() {
  require_tool docker
  docker run --rm --privileged \
    -v "$OUTPUT_DIR_ABS:/work" \
    "$DOCKER_IMAGE" \
    bash -lc '
      set -euo pipefail

      require_tool() {
        command -v "$1" >/dev/null 2>&1 || {
          echo "error: docker image is missing required tool: $1" >&2
          exit 1
        }
      }

      json_escape() {
        awk '\''
          BEGIN { ORS = "" }
          {
            gsub(/\\/, "\\\\")
            gsub(/"/, "\\\"")
            if (NR > 1) {
              printf "\\n"
            }
            printf "%s", $0
          }
        '\'' <<<"$1"
      }

      json_string() {
        printf "\"%s\"" "$(json_escape "$1")"
      }

      require_tool losetup
      require_tool mount
      require_tool umount
	      require_tool blkid
	      require_tool find
	      require_tool cp
	      require_tool partx
	      require_tool sed
	      require_tool sort
	      require_tool awk

	      mkdir -p /mnt/bridgevm-root /mnt/bridgevm-boot
	      loop_device="$(losetup --find --show --partscan /work/root.raw)"
	      root_mounted=false
	      boot_mounted=false
	      cleanup() {
	        if [[ "$boot_mounted" == "true" ]]; then
	          umount /mnt/bridgevm-boot >/dev/null 2>&1 || true
	        fi
	        if [[ "$root_mounted" == "true" ]]; then
	          umount /mnt/bridgevm-root >/dev/null 2>&1 || true
	        fi
	        if [[ -n "${loop_device:-}" ]]; then
	          losetup -d "$loop_device" >/dev/null 2>&1 || true
	        fi
	      }
	      trap cleanup EXIT

	      sleep 0.2
	      candidates_file="/tmp/bridgevm-partitions.tsv"
	      : >"$candidates_file"
	      for candidate in "${loop_device}"p*; do
	        if [[ -b "$candidate" ]]; then
	          partition="$(basename "$candidate" | sed -n '"'"'s/^.*p\([0-9][0-9]*\)$/\1/p; s/^.*[^0-9]\([0-9][0-9]*\)$/\1/p'"'"' | head -1)"
	          printf "device|%s|%s|\n" "$candidate" "$partition" >>"$candidates_file"
	        fi
	      done
	      if [[ ! -s "$candidates_file" ]]; then
	        losetup -d "$loop_device" >/dev/null 2>&1 || true
	        loop_device=""
	        partx --raw --output NR,START,SECTORS /work/root.raw | awk "NR > 1 { printf \"offset|/work/root.raw|%s|%d\\n\", \$1, \$2 * 512 }" >"$candidates_file"
	      fi

	      mount_candidate() {
	        local kind="$1"
	        local source="$2"
	        local offset="$3"
	        local target="$4"
	        if [[ "$kind" == "offset" ]]; then
	          mount -o "ro,loop,offset=$offset" "$source" "$target"
	        else
	          mount -o ro "$source" "$target"
	        fi
	      }

	      blkid_value() {
	        local kind="$1"
	        local source="$2"
	        local offset="$3"
	        local key="$4"
	        if [[ "$kind" == "offset" ]]; then
	          blkid -p -o value -s "$key" -O "$offset" "$source" 2>/dev/null || true
	        else
	          blkid -s "$key" -o value "$source" 2>/dev/null || true
	        fi
	      }

	      root_device=""
	      root_kind=""
	      root_source=""
	      root_offset=""
	      root_partition=""
	      while IFS="|" read -r kind source partition offset; do
	        if mount_candidate "$kind" "$source" "$offset" /mnt/bridgevm-root >/dev/null 2>&1; then
	          root_mounted=true
	          if [[ -f /mnt/bridgevm-root/etc/os-release ]] && grep -Eq '"'"'(^ID="?ubuntu"?$|^NAME="?Ubuntu)'"'"' /mnt/bridgevm-root/etc/os-release; then
	            root_device="$source"
	            root_kind="$kind"
	            root_source="$source"
	            root_offset="$offset"
	            root_partition="$partition"
	            break
	          fi
	          umount /mnt/bridgevm-root >/dev/null 2>&1 || true
	          root_mounted=false
	        fi
	      done <"$candidates_file"

	      [[ -n "$root_device" ]] || {
	        echo "error: could not locate an Ubuntu root filesystem in /work/root.raw" >&2
	        exit 1
	      }

		      root_modules_file="/tmp/bridgevm-root-modules.txt"
		      : >"$root_modules_file"
		      if [[ -d /mnt/bridgevm-root/lib/modules ]]; then
		        for module_dir in /mnt/bridgevm-root/lib/modules/*; do
		          [[ -d "$module_dir" ]] && basename "$module_dir"
		        done | sort >"$root_modules_file"
		      fi

		      desktop_stack=false
		      if [[ -e /mnt/bridgevm-root/usr/sbin/gdm3 || -e /mnt/bridgevm-root/usr/sbin/lightdm || -e /mnt/bridgevm-root/usr/sbin/sddm || -e /mnt/bridgevm-root/etc/systemd/system/display-manager.service ]]; then
		        desktop_stack=true
		      fi

		      module_version_exists() {
		        grep -Fxq "$1" "$root_modules_file"
		      }

		      select_kernel_from_dir() {
		        local search_dir="$1"
		        while IFS= read -r candidate; do
		          version="$(basename "$candidate")"
		          version="${version#vmlinuz-}"
		          if module_version_exists "$version"; then
		            selected_kernel="$candidate"
		            selected_version="$version"
		            return 0
	          fi
	        done < <(find "$search_dir" -maxdepth 1 -type f -name "vmlinuz-*" | sort -Vr)
	        candidate="$(find "$search_dir" -maxdepth 1 -type f -name "vmlinuz-*" | sort -Vr | head -1)"
	        if [[ -n "$candidate" ]]; then
	          version="$(basename "$candidate")"
	          version="${version#vmlinuz-}"
	          selected_kernel="$candidate"
	          selected_version="$version"
	          return 0
	        fi
	        return 1
	      }

	      selected_kernel=""
	      selected_version=""
	      selected_boot_base="/mnt/bridgevm-root/boot"
	      selected_kernel_guest_prefix="/boot"
	      select_kernel_from_dir /mnt/bridgevm-root/boot 2>/dev/null || true

	      boot_partition=""
		      boot_uuid=""
		      boot_fstype=""
		      if [[ -z "$selected_kernel" ]]; then
		        if [[ "$root_mounted" == "true" ]]; then
		          umount /mnt/bridgevm-root >/dev/null 2>&1 || true
		          root_mounted=false
		        fi
		        while IFS="|" read -r kind source partition offset; do
		          [[ "$kind" == "$root_kind" && "$source" == "$root_source" && "$offset" == "$root_offset" ]] && continue
	          if mount_candidate "$kind" "$source" "$offset" /mnt/bridgevm-boot >/dev/null 2>&1; then
	            boot_mounted=true
	            if select_kernel_from_dir /mnt/bridgevm-boot 2>/dev/null; then
	              selected_boot_base="/mnt/bridgevm-boot"
	              selected_kernel_guest_prefix="/boot"
	              boot_partition="$partition"
	              boot_uuid="$(blkid_value "$kind" "$source" "$offset" UUID)"
	              boot_fstype="$(blkid_value "$kind" "$source" "$offset" TYPE)"
	              break
	            fi
	            umount /mnt/bridgevm-boot >/dev/null 2>&1 || true
	            boot_mounted=false
	          fi
	        done <"$candidates_file"
	      fi

	      [[ -n "$selected_kernel" ]] || {
	        echo "error: no /boot/vmlinuz-* kernel found in Ubuntu root or boot partition" >&2
	        exit 1
	      }

	      selected_initrd=""
	      for candidate in \
	        "$selected_boot_base/initrd.img-$selected_version" \
	        "$selected_boot_base/initrd-$selected_version" \
	        "$selected_boot_base/initramfs-$selected_version.img"; do
	        if [[ -f "$candidate" ]]; then
	          selected_initrd="$candidate"
	          break
	        fi
	      done
	      if [[ -z "$selected_initrd" ]]; then
	        selected_initrd="$(find "$selected_boot_base" -maxdepth 1 -type f \( -name "initrd.img-*" -o -name "initrd-*" -o -name "initramfs-*.img" \) | sort -Vr | head -1)"
	      fi
	      [[ -n "$selected_initrd" ]] || {
	        echo "error: no matching initrd found in Ubuntu root filesystem" >&2
	        exit 1
	      }

	      root_uuid="$(blkid_value "$root_kind" "$root_source" "$root_offset" UUID)"
		      root_label="$(blkid_value "$root_kind" "$root_source" "$root_offset" LABEL)"
		      root_fstype="$(blkid_value "$root_kind" "$root_source" "$root_offset" TYPE)"
		      modules_match=false
		      module_version_exists "$selected_version" && modules_match=true

		      cp "$selected_kernel" /work/vmlinuz
	      cp "$selected_initrd" /work/initrd

	      selected_kernel_guest="$selected_kernel_guest_prefix/$(basename "$selected_kernel")"
	      selected_initrd_guest="$selected_kernel_guest_prefix/$(basename "$selected_initrd")"

	      {
	        printf "{\n"
	        if [[ "$root_kind" == "offset" ]]; then
	          printf "  \"root_device_host\": %s,\n" "$(json_string "$root_source@$root_offset")"
	        else
	          printf "  \"root_device_host\": %s,\n" "$(json_string "$root_device")"
	        fi
	        if [[ -n "$root_partition" ]]; then
	          printf "  \"root_partition\": %s,\n" "$(json_string "$root_partition")"
	        else
	          printf "  \"root_partition\": null,\n"
	        fi
	        printf "  \"root_offset_bytes\": %s,\n" "${root_offset:-0}"
	        printf "  \"root_uuid\": %s,\n" "$(json_string "$root_uuid")"
	        printf "  \"root_label\": %s,\n" "$(json_string "$root_label")"
	        printf "  \"root_fstype\": %s,\n" "$(json_string "$root_fstype")"
	        if [[ -n "$boot_partition" ]]; then
	          printf "  \"boot_partition\": %s,\n" "$(json_string "$boot_partition")"
	        else
	          printf "  \"boot_partition\": null,\n"
	        fi
	        printf "  \"boot_uuid\": %s,\n" "$(json_string "$boot_uuid")"
	        printf "  \"boot_fstype\": %s,\n" "$(json_string "$boot_fstype")"
	        printf "  \"kernel_version\": %s,\n" "$(json_string "$selected_version")"
	        printf "  \"kernel_path_in_guest\": %s,\n" "$(json_string "$selected_kernel_guest")"
        printf "  \"initrd_path_in_guest\": %s,\n" "$(json_string "$selected_initrd_guest")"
        printf "  \"modules_match\": %s,\n" "$modules_match"
        printf "  \"desktop_stack_detected\": %s\n" "$desktop_stack"
        printf "}\n"
      } >/work/extraction.json
    '
}

preflight_backend() {
  local selected_backend="$1"
  case "$selected_backend" in
    docker-offline)
      require_tool docker
      if [[ "$ALLOW_DOCKER_PULL" != "1" ]]; then
        docker image inspect "$DOCKER_IMAGE" >/dev/null || {
          echo "error: Docker image is not available locally: $DOCKER_IMAGE" >&2
          echo "hint: pass --allow-docker-pull to permit Docker to pull it, or pass --docker-image with a local image that has losetup/mount/blkid." >&2
          exit 1
        }
      fi
      ;;
  esac
}

write_artifacts_json() {
  local source_format="$1"
  local source_virtual_size="$2"
  local selected_backend="$3"
  local kernel_transform="$4"
  local source_sha
  local raw_sha
  local kernel_sha
  local initrd_sha
  local raw_size
  source_sha="$(file_sha256 "$SOURCE_IMAGE_ABS")"
  raw_sha="$(file_sha256 "$RAW_DISK")"
  kernel_sha="$(file_sha256 "$KERNEL")"
  initrd_sha="$(file_sha256 "$INITRD")"
  raw_size="$(file_size_bytes "$RAW_DISK")"

  local root_uuid
  local root_partition
  local kernel_version
  local desktop_stack
  root_uuid="$(jq -r '.root_uuid // ""' "$EXTRACTION_JSON")"
  root_partition="$(jq -r '.root_partition // ""' "$EXTRACTION_JSON")"
  kernel_version="$(jq -r '.kernel_version // ""' "$EXTRACTION_JSON")"
  desktop_stack="$(jq -r '.desktop_stack_detected // false' "$EXTRACTION_JSON")"

  local root_cmdline
  if [[ -n "$root_uuid" ]]; then
    root_cmdline="root=UUID=$root_uuid"
  elif [[ -n "$root_partition" ]]; then
    root_cmdline="root=/dev/vda$root_partition"
  else
    root_cmdline="root=/dev/vda"
  fi

  local kernel_cmdline="console=hvc0 $root_cmdline rw"
  if [[ "$desktop_stack" == "true" ]]; then
    kernel_cmdline+=" systemd.unit=graphical.target"
  fi

  local tmp
  tmp="$(mktemp "$ARTIFACTS_JSON.tmp.XXXXXX")"
  jq -n \
    --arg source_image "$SOURCE_IMAGE_ABS" \
    --arg source_sha "$source_sha" \
    --arg source_format "$source_format" \
    --arg source_virtual_size "$source_virtual_size" \
    --arg output_dir "$OUTPUT_DIR_ABS" \
    --arg raw_disk "$RAW_DISK" \
    --arg raw_sha "$raw_sha" \
    --arg raw_size "$raw_size" \
    --arg kernel "$KERNEL" \
    --arg kernel_sha "$kernel_sha" \
	    --arg initrd "$INITRD" \
	    --arg initrd_sha "$initrd_sha" \
	    --arg backend "$selected_backend" \
	    --arg disk_size "$DISK_SIZE" \
	    --arg kernel_transform "$kernel_transform" \
	    --arg kernel_cmdline "$kernel_cmdline" \
	    --slurpfile extraction "$EXTRACTION_JSON" \
	    '{
      schema_version: 1,
      guest: { os: "ubuntu", arch: "arm64" },
      source: {
        image: $source_image,
        sha256: $source_sha,
        format: $source_format,
        virtual_size: ($source_virtual_size | tonumber? // null)
      },
      output_dir: $output_dir,
      conversion: {
        qemu_img_offline_only: true,
        requested_disk_size: (if $disk_size == "" then null else $disk_size end)
      },
      raw_disk: {
        path: $raw_disk,
        sha256: $raw_sha,
        size_bytes: ($raw_size | tonumber),
        format: "raw"
      },
	      kernel: {
	        path: $kernel,
	        sha256: $kernel_sha,
	        apple_vz_transform: $kernel_transform
	      },
      initrd: { path: $initrd, sha256: $initrd_sha },
      extraction: $extraction[0],
      kernel_version: $extraction[0].kernel_version,
      root_partition: $extraction[0].root_partition,
      root_uuid: $extraction[0].root_uuid,
      root_fstype: $extraction[0].root_fstype,
      modules_match: $extraction[0].modules_match,
      desktop_stack_detected: $extraction[0].desktop_stack_detected,
      kernel_command_line: $kernel_cmdline,
      prep_backend: $backend,
      runtime: {
        qemu_system_used: false,
        apple_vz_started: false,
        gui_spawned: false
      }
    }' >"$tmp"
  mv "$tmp" "$ARTIFACTS_JSON"
  printf '%s\n' "$kernel_cmdline"
}

if [[ "$DRY_RUN" == "1" ]]; then
  require_tool qemu-img
  require_tool jq
  info_json="$(qemu-img info --output=json "$SOURCE_IMAGE_ABS")"
  source_format="$(jq -r '.format // ""' <<<"$info_json")"
  source_virtual_size="$(jq -r '."virtual-size" // .virtual_size // ""' <<<"$info_json")"
  selected_backend="$(select_backend)"
  cat <<EOF
# Ubuntu Arm64 Apple VZ boot artifact plan
# source_image: $SOURCE_IMAGE_ABS
# source_format: $source_format
# source_virtual_size: $source_virtual_size
# output_dir: $OUTPUT_DIR_ABS
# raw_disk: $RAW_DISK
# artifacts_json: $ARTIFACTS_JSON
# selected_backend: $selected_backend
# docker_image: $DOCKER_IMAGE
# allow_docker_pull: $ALLOW_DOCKER_PULL
# qemu_img_use: offline inspection/conversion only
# qemu_system_runtime: false
EOF
  print_exports
  exit 0
fi

require_tool qemu-img
require_tool jq
require_tool cp
require_tool mktemp
require_tool mv
if ! have_tool shasum && ! have_tool sha256sum; then
  echo "error: neither shasum nor sha256sum is available" >&2
  exit 1
fi
[[ -f "$SOURCE_IMAGE_ABS" ]] || {
  echo "error: source image does not exist: $SOURCE_IMAGE_ABS" >&2
  exit 1
}

selected_backend="$(select_backend)"
[[ "$selected_backend" != "none" ]] || {
  echo "error: no artifact extraction backend is available; install Docker or pass --prep-backend docker-offline" >&2
  exit 1
}
preflight_backend "$selected_backend"

mkdir -p "$OUTPUT_DIR_ABS"
if [[ "$FORCE" != "1" ]]; then
  for output in "$RAW_DISK" "$KERNEL" "$INITRD" "$EXTRACTION_JSON" "$ARTIFACTS_JSON"; do
    [[ ! -e "$output" ]] || {
      echo "error: output already exists; pass --force to replace: $output" >&2
      exit 1
    }
  done
fi

rm -f "$RAW_DISK" "$KERNEL" "$INITRD" "$EXTRACTION_JSON" "$ARTIFACTS_JSON"

info_json="$(qemu-img info --output=json "$SOURCE_IMAGE_ABS")"
source_format="$(jq -r '.format // ""' <<<"$info_json")"
source_virtual_size="$(jq -r '."virtual-size" // .virtual_size // ""' <<<"$info_json")"
[[ -n "$source_format" ]] || {
  echo "error: qemu-img did not report a source image format" >&2
  exit 1
}

case "$source_format" in
  raw)
    cp "$SOURCE_IMAGE_ABS" "$RAW_DISK"
    ;;
  qcow2)
    qemu-img convert -O raw "$SOURCE_IMAGE_ABS" "$RAW_DISK"
    ;;
  *)
    echo "error: unsupported source image format for Apple VZ artifact preparation: $source_format" >&2
    exit 1
    ;;
esac

if [[ -n "$DISK_SIZE" ]]; then
  qemu-img resize -f raw "$RAW_DISK" "$DISK_SIZE"
fi

case "$selected_backend" in
  docker-offline)
    docker_extract
    ;;
esac

[[ -s "$RAW_DISK" ]] || { echo "error: raw disk output missing: $RAW_DISK" >&2; exit 1; }
[[ -s "$KERNEL" ]] || { echo "error: kernel output missing: $KERNEL" >&2; exit 1; }
[[ -s "$INITRD" ]] || { echo "error: initrd output missing: $INITRD" >&2; exit 1; }
[[ -s "$EXTRACTION_JSON" ]] || { echo "error: extraction metadata missing: $EXTRACTION_JSON" >&2; exit 1; }

kernel_transform="$(normalize_kernel_for_apple_vz)"
kernel_cmdline="$(write_artifacts_json "$source_format" "$source_virtual_size" "$selected_backend" "$kernel_transform")"

cat <<EOF
# Prepared Ubuntu Arm64 Apple VZ boot artifacts in $OUTPUT_DIR_ABS
# Source format: $source_format
# Backend: $selected_backend
# Kernel transform: $kernel_transform
# Kernel command line: $kernel_cmdline
# Artifacts: $ARTIFACTS_JSON
EOF
print_exports "$kernel_cmdline"

if [[ -n "$STAGE_NAME" ]]; then
  scripts/stage-vz-ubuntu-desktop-vm.sh \
    --store "$STORE" \
    --name "$STAGE_NAME" \
    --fixture-dir "$OUTPUT_DIR_ABS" \
    --kernel-command-line "$kernel_cmdline"
fi
