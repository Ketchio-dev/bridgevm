positive_integer() {
  [[ "$1" =~ ^[1-9][0-9]*$ ]]
}

nonnegative_integer() {
  [[ "$1" =~ ^[0-9]+$ ]]
}

truthy_env_value() {
  local value
  value="${1:-}"
  value="${value#"${value%%[![:space:]]*}"}"
  value="${value%"${value##*[![:space:]]}"}"
  value="$(printf '%s' "$value" | tr '[:upper:]' '[:lower:]')"
  case "$value" in
    1|true|yes|on) return 0 ;;
    *) return 1 ;;
  esac
}

smp_cpu_count() {
  positive_integer "$1" || return 1
  (( ${#1} <= 3 )) || return 1
  (( 10#$1 <= 123 ))
}

boot_timer_ramfb_ms() {
  positive_integer "$1" || return 1
  (( ${#1} <= 5 )) || return 1
  (( 10#$1 >= 100 && 10#$1 <= 60000 ))
}

u64_literal() {
  local value="$1"
  local trimmed
  case "$value" in
    0x*|0X*)
      [[ "${value#??}" =~ ^[0-9a-fA-F]{1,16}$ ]]
      ;;
    *)
      [[ "$value" =~ ^[0-9]+$ ]] || return 1
      trimmed="${value#"${value%%[!0]*}"}"
      [[ -n "$trimmed" ]] || trimmed="0"
      (( ${#trimmed} < 20 )) && return 0
      [[ ${#trimmed} -eq 20 && "$trimmed" < "18446744073709551616" ]]
      ;;
  esac
}

normalize_virtio_gpu_device_id() {
  local value="$1"
  local upper
  upper="$(printf '%s\n' "$value" | tr '[:lower:]' '[:upper:]')"
  upper="${upper#0X}"
  case "$upper" in
    1050|10F7) printf '%s\n' "$upper" ;;
    *) return 1 ;;
  esac
}

ramfb_sample_list() {
  [[ "$1" =~ ^[1-9][0-9]*(,[1-9][0-9]*)*$ ]] || return 1

  local sample
  local count=0
  local old_ifs="$IFS"
  IFS=,
  for sample in $1; do
    if (( sample > 120000 )); then
      IFS="$old_ifs"
      return 1
    fi
    count=$((count + 1))
    if (( count > 16 )); then
      IFS="$old_ifs"
      return 1
    fi
  done
  IFS="$old_ifs"
}

setup_input_actions_list() {
  (( ${#1} <= 128 )) || return 1

  local token
  local normalized
  local text
  local count=0
  local -a tokens
  local old_ifs="$IFS"
  IFS=,
  read -r -a tokens <<< "$1"
  IFS="$old_ifs"
  for token in "${tokens[@]}"; do
    token="${token#"${token%%[![:space:]]*}"}"
    token="${token%"${token##*[![:space:]]}"}"
    [[ -n "$token" ]] || continue
    normalized="$(printf '%s' "$token" | tr '[:upper:]' '[:lower:]')"
    case "$normalized" in
      tab|enter|space|win+r|lgui+r)
        count=$((count + 1))
        ;;
      text:*)
        case "$token" in
          text:*) text="${token#text:}" ;;
          *) return 1 ;;
        esac
        [[ "$text" =~ ^[a-z0-9/.-]+$ ]] || return 1
        count=$((count + ${#text}))
        ;;
      *)
        return 1
        ;;
    esac
    if (( count > 32 )); then
      return 1
    fi
  done
  (( count > 0 ))
}

setup_input_marker_value() {
  [[ -n "$1" ]] || return 1
  (( ${#1} <= 96 ))
}

setup_input_fire_delay_ms() {
  nonnegative_integer "$1" || return 1
  (( 10#$1 <= 600000 ))
}

pointer_input_actions_list() {
  (( ${#1} <= 128 )) || return 1

  local token
  local normalized
  local position
  local x
  local y
  local count=0
  for token in ${1//,/ }; do
    normalized="$(printf '%s' "$token" | tr '[:upper:]' '[:lower:]')"
    case "$normalized" in
      move:*|click:*)
        position="${normalized#*:}"
        if [[ "$position" != "center" ]]; then
          [[ "$position" =~ ^[0-9]+x[0-9]+$ ]] || return 1
          x="${position%x*}"
          y="${position#*x}"
          (( 10#$x <= 32767 && 10#$y <= 32767 )) || return 1
        fi
        count=$((count + 1))
        ;;
      *)
        return 1
        ;;
    esac
    if (( count > 16 )); then
      return 1
    fi
  done
  (( count > 0 ))
}

absolute_media_path() {
  local dir
  local base
  dir="$(dirname "$1")"
  base="$(basename "$1")"
  printf '%s/%s\n' "$(cd "$dir" && pwd -P)" "$base"
}

absolute_path_from() {
  local base_dir="$1"
  local path="$2"
  case "$path" in
    /*) printf '%s\n' "$path" ;;
    *) printf '%s/%s\n' "$base_dir" "$path" ;;
  esac
}

absolutize_installed_boot_paths() {
  local invocation_dir="$1"
  [[ -z "$TARGET" ]] || TARGET="$(absolute_path_from "$invocation_dir" "$TARGET")"
  [[ -z "$PLACEHOLDER_NSID1" ]] || PLACEHOLDER_NSID1="$(absolute_path_from "$invocation_dir" "$PLACEHOLDER_NSID1")"
  [[ -z "$VARS" ]] || VARS="$(absolute_path_from "$invocation_dir" "$VARS")"
  [[ -z "$EVIDENCE_DIR" ]] || EVIDENCE_DIR="$(absolute_path_from "$invocation_dir" "$EVIDENCE_DIR")"
  [[ -z "$VIRTIO_GPU_TRACE_JSONL" ]] || VIRTIO_GPU_TRACE_JSONL="$(absolute_path_from "$invocation_dir" "$VIRTIO_GPU_TRACE_JSONL")"
  [[ -z "$VIOGPU3D_DIR" ]] || VIOGPU3D_DIR="$(absolute_path_from "$invocation_dir" "$VIOGPU3D_DIR")"
}

media_identity() {
  stat -L -f '%d:%i' "$1" 2>/dev/null
}

matches_preserved_source_media_identity() {
  local path_identity
  local preserved_path
  local preserved_identity
  path_identity="$(media_identity "$1")" || return 1
  for preserved_path in \
    /tmp/bridgevm-c3-unattend-target.raw \
    /private/tmp/bridgevm-c3-unattend-target.raw \
    /tmp/bridgevm-c3-unattend-vars.fd \
    /private/tmp/bridgevm-c3-unattend-vars.fd \
    /tmp/bridgevm-c3-placeholder-nsid1.raw \
    /private/tmp/bridgevm-c3-placeholder-nsid1.raw
  do
    [[ -e "$preserved_path" ]] || continue
    preserved_identity="$(media_identity "$preserved_path")" || continue
    [[ "$path_identity" == "$preserved_identity" ]] && return 0
  done
  return 1
}

path_has_parent_component() {
  case "$1" in
    ..|../*|*/..|*/../*) return 0 ;;
    *) return 1 ;;
  esac
}

require_not_preserved_source_media() {
  local label="$1"
  local path
  if path_has_parent_component "$2"; then
    echo "FAIL: $label path must not contain '..' components: $2" >&2
    exit 2
  fi
  if matches_preserved_source_media_identity "$2"; then
    echo "FAIL: $label path is preserved source media or an alias; use a cloned /tmp/bridgevm-g*-* image: $2" >&2
    exit 2
  fi
  path="$(absolute_media_path "$2")"
  case "$path" in
    /tmp/bridgevm-c3-unattend-target.raw|/private/tmp/bridgevm-c3-unattend-target.raw|\
    /tmp/bridgevm-c3-unattend-vars.fd|/private/tmp/bridgevm-c3-unattend-vars.fd|\
    /tmp/bridgevm-c3-placeholder-nsid1.raw|/private/tmp/bridgevm-c3-placeholder-nsid1.raw)
      echo "FAIL: $label path matches preserved source media; use a cloned /tmp/bridgevm-g*-* image: $2" >&2
      exit 2
      ;;
  esac
}
