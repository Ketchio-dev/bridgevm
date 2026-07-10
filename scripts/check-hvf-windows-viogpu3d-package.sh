#!/usr/bin/env bash
# Validate a Windows ARM64 viogpu3d package before offline injection.
set -euo pipefail

VIOGPU3D_DIR="${VIOGPU3D_DIR:-}"
VIOGPU3D_PROTOCOL="${VIOGPU3D_PROTOCOL:-auto}"
VIOGPU3D_MANIFEST="${VIOGPU3D_MANIFEST:-}"
VIOGPU3D_PCI_DEVICE_ID="${VIOGPU3D_PCI_DEVICE_ID:-}"
VIOGPU3D_PROVENANCE="${VIOGPU3D_PROVENANCE:-}"
VIOGPU3D_REQUIRE_RENDER_CANDIDATE="${VIOGPU3D_REQUIRE_RENDER_CANDIDATE:-0}"
VIOGPU3D_SOURCE_REPO="${VIOGPU3D_SOURCE_REPO:-<unknown>}"
VIOGPU3D_SOURCE_REF="${VIOGPU3D_SOURCE_REF:-<unknown>}"
VIOGPU3D_BUILD_ID="${VIOGPU3D_BUILD_ID:-<unknown>}"
VIOGPU3D_SIGNING_CERT="${VIOGPU3D_SIGNING_CERT:-<unknown>}"
VIOGPU3D_PROVENANCE_EXPLICIT=0
[[ -z "$VIOGPU3D_PROVENANCE" ]] || VIOGPU3D_PROVENANCE_EXPLICIT=1
loaded_provenance="<none>"
provenance_protocol_loaded=0

usage() {
  cat >&2 <<'EOF'
usage: VIOGPU3D_DIR=/path/to/viogpu3d-package scripts/check-hvf-windows-viogpu3d-package.sh
       scripts/check-hvf-windows-viogpu3d-package.sh [--manifest PATH] [--provenance PATH] [--pci-device-id 1050|10f7] [--require-render-candidate] /path/to/viogpu3d-package

Environment:
  VIOGPU3D_DIR       Directory containing viogpu3d .inf/.sys/.cat files. May
                     also be passed as the first positional argument.
  VIOGPU3D_PROTOCOL  auto, venus, or virgl. Default: auto. Use venus/virgl only
                     after auditing the package/source when auto is ambiguous.
  VIOGPU3D_MANIFEST  Optional path for a text manifest containing package file
                     hashes, sizes, PE machine fields, and source metadata.
  VIOGPU3D_PROVENANCE
                     Optional provenance env file. When unset, the checker
                     auto-loads bridgevm-package-provenance.env from the package
                     directory if present. Explicit --provenance or env path
                     must exist.
  VIOGPU3D_PCI_DEVICE_ID
                     Optional expected virtio-gpu PCI device id, 1050 or 10f7.
                     When unset, either id is accepted and recorded.
  VIOGPU3D_REQUIRE_RENDER_CANDIDATE
                     Set to 1 to require a VirGL package with ARM64 user-mode
                     DLLs plus active INF
                     UserModeDriverName, OpenGLDriverName, OpenGLVersion,
                     OpenGLFlags, and InstalledDisplayDrivers registrations,
                     with the registered DLLs actively copied to DirID 11 and
                     every source named by the selected DDInstall CopyFiles
                     path present in the package.
                     Default: 0, so KMD-only packages remain valid injection
                     candidates.
  VIOGPU3D_SOURCE_REPO / VIOGPU3D_SOURCE_REF / VIOGPU3D_BUILD_ID /
  VIOGPU3D_SIGNING_CERT
                     Optional provenance fields copied into the manifest.
EOF
}

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

read_bytes_dec() {
  local path="$1"
  local offset="$2"
  local count="$3"
  LC_ALL=C od -An -tu1 -j "$offset" -N "$count" "$path" 2>/dev/null
}

pe_machine_hex() {
  local path="$1"
  local label="$2"
  local -a bytes
  local -a sig
  local -a machine
  local pe_offset

  read -r -a bytes <<<"$(read_bytes_dec "$path" 0 2)"
  (( ${#bytes[@]} == 2 )) || fail "$label is too small to be a PE image: $path"
  [[ "${bytes[0]}" == "77" && "${bytes[1]}" == "90" ]] || {
    fail "$label is not a PE/MZ image: $path"
  }

  read -r -a bytes <<<"$(read_bytes_dec "$path" 60 4)"
  (( ${#bytes[@]} == 4 )) || fail "$label missing PE header offset: $path"
  pe_offset=$(( bytes[0] + bytes[1] * 256 + bytes[2] * 65536 + bytes[3] * 16777216 ))

  read -r -a sig <<<"$(read_bytes_dec "$path" "$pe_offset" 4)"
  (( ${#sig[@]} == 4 )) || fail "$label missing PE signature: $path"
  [[ "${sig[0]}" == "80" && "${sig[1]}" == "69" && "${sig[2]}" == "0" && "${sig[3]}" == "0" ]] || {
    fail "$label has an invalid PE signature: $path"
  }

  read -r -a machine <<<"$(read_bytes_dec "$path" "$((pe_offset + 4))" 2)"
  (( ${#machine[@]} == 2 )) || fail "$label missing PE machine field: $path"
  printf '0x%02x%02x\n' "${machine[1]}" "${machine[0]}"
}

pe_arm64_machine_gate() {
  local path="$1"
  local label="$2"
  local machine_hex

  machine_hex="$(pe_machine_hex "$path" "$label")"
  [[ "$machine_hex" == "0xaa64" ]] || {
    fail "$label is not ARM64 PE (machine=$machine_hex): $path"
  }
}

reject_whitespace_path() {
  case "$2" in
    *[[:space:]]*) fail "$1 path contains whitespace, unsupported by DRIVER_DIRS: $2" ;;
  esac
}

reject_windows_unsafe_basename() {
  local path="$1"
  local name="${path##*/}"

  if [[ -z "$name" || "$name" == "." || "$name" == ".." ||
        "$name" == *$'\n'* || "$name" == *$'\r'* ||
        "$name" == *' ' || "$name" == *'.' ]] ||
    LC_ALL=C printf '%s' "$name" | grep -Eq '[<>:"/\\|?*]|[[:cntrl:]]'; then
    fail "package filename is not Windows-safe: $name"
  fi
}

apply_provenance_default() {
  local key="$1"
  local value="$2"
  case "$key" in
    VIOGPU3D_SOURCE_REPO)
      [[ "$VIOGPU3D_SOURCE_REPO" != "<unknown>" ]] || VIOGPU3D_SOURCE_REPO="$value"
      ;;
    VIOGPU3D_SOURCE_REF)
      [[ "$VIOGPU3D_SOURCE_REF" != "<unknown>" ]] || VIOGPU3D_SOURCE_REF="$value"
      ;;
    VIOGPU3D_BUILD_ID)
      [[ "$VIOGPU3D_BUILD_ID" != "<unknown>" ]] || VIOGPU3D_BUILD_ID="$value"
      ;;
    VIOGPU3D_SIGNING_CERT)
      [[ "$VIOGPU3D_SIGNING_CERT" != "<unknown>" ]] || VIOGPU3D_SIGNING_CERT="$value"
      ;;
    VIOGPU3D_PROTOCOL)
      if [[ "$VIOGPU3D_PROTOCOL" == "auto" && -n "$value" ]]; then
        VIOGPU3D_PROTOCOL="$value"
        provenance_protocol_loaded=1
      fi
      ;;
    VIOGPU3D_PCI_DEVICE_ID)
      [[ -n "$VIOGPU3D_PCI_DEVICE_ID" || -z "$value" ]] || VIOGPU3D_PCI_DEVICE_ID="$value"
      ;;
    *)
      fail "unsupported provenance key $key in $loaded_provenance"
      ;;
  esac
}

load_provenance_defaults() {
  local path="$1"
  local explicit="$2"
  local line
  local key
  local value

  if [[ ! -f "$path" ]]; then
    [[ "$explicit" == "0" ]] || fail "provenance file not found: $path"
    return 0
  fi

  loaded_provenance="$path"
  while IFS= read -r line || [[ -n "$line" ]]; do
    line="${line%$'\r'}"
    [[ -n "$line" ]] || continue
    [[ "$line" != \#* ]] || continue
    [[ "$line" == *=* ]] || fail "invalid provenance line in $path: $line"
    key="${line%%=*}"
    value="${line#*=}"
    [[ "$key" =~ ^[A-Z0-9_]+$ ]] || fail "invalid provenance key in $path: $key"
    apply_provenance_default "$key" "$value"
  done < "$path"
}

normalize_pci_device_id() {
  local value="$1"
  local upper
  upper="$(printf '%s\n' "$value" | tr '[:lower:]' '[:upper:]')"
  upper="${upper#0X}"
  case "$upper" in
    1050|10F7) printf '%s\n' "$upper" ;;
    *) fail "VIOGPU3D_PCI_DEVICE_ID must be 1050 or 10f7, got: $value" ;;
  esac
}

append_unique_hwid() {
  local id="$1"
  local existing
  for existing in ${viogpu3d_hwids:-}; do
    [[ "$existing" == "$id" ]] && return 0
  done
  viogpu3d_hwids="${viogpu3d_hwids:+$viogpu3d_hwids }$id"
}

detect_hwid_device_ids() {
  local inf
  local id
  viogpu3d_hwids=""
  for inf in "${viogpu3d_infs[@]}"; do
    while IFS= read -r id; do
      [[ -n "$id" ]] || continue
      case "$id" in
        1050|10F7) append_unique_hwid "$id" ;;
      esac
    done < <(
      LC_ALL=C awk '
        function strip_inf_comment(value,    i, char, quoted, percent_token, result) {
          quoted = 0
          percent_token = 0
          result = ""
          for (i = 1; i <= length(value); i++) {
            char = substr(value, i, 1)
            if (char == "\"") quoted = !quoted
            else if (char == "%" && !quoted) percent_token = !percent_token
            else if (char == ";" && !quoted && !percent_token) break
            result = result char
          }
          return result
        }
        function raw_component(value, component,    offset, relative, position, before, after) {
          value = tolower(value)
          component = tolower(component)
          offset = 1
          while (offset <= length(value)) {
            relative = index(substr(value, offset), component)
            if (relative == 0) return 0
            position = offset + relative - 1
            before = position == 1 ? "" : substr(value, position - 1, 1)
            after = substr(value, position + length(component), 1)
            if ((before == "" || before == "\\" || before == "&") &&
                (after == "" || after == "&" || after ~ /[[:space:],;\"]/) ) return 1
            offset = position + 1
          }
          return 0
        }
        {
          count = split(strip_inf_comment($0), field, ",")
          for (i = 1; i <= count; i++) {
            if (raw_component(field[i], "ven_1af4") && raw_component(field[i], "dev_1050")) print "1050"
            if (raw_component(field[i], "ven_1af4") && raw_component(field[i], "dev_10f7")) print "10F7"
          }
        }
      ' "$inf" 2>/dev/null || true
    )
  done
}

join_hwids() {
  local sep=""
  local id
  for id in ${viogpu3d_hwids:-}; do
    printf '%sPCI\\VEN_1AF4&DEV_%s' "$sep" "$id"
    sep=","
  done
  printf '\n'
}

hwid_contains() {
  local expected="$1"
  local id
  for id in ${viogpu3d_hwids:-}; do
    [[ "$id" == "$expected" ]] && return 0
  done
  return 1
}

detect_protocol() {
  local venus_hit=0
  local virgl_hit=0

  if grep -Eaiq 'venus|vulkan|capset[^[:alnum:]]*4' "${scan_files[@]}" 2>/dev/null; then
    venus_hit=1
  fi
  if grep -Eaiq 'virgl|d3d10|d3d10umd|wgl|opengl|gallium' "${scan_files[@]}" 2>/dev/null; then
    virgl_hit=1
  fi

  if (( venus_hit == 1 && virgl_hit == 0 )); then
    printf 'venus\n'
  elif (( virgl_hit == 1 && venus_hit == 0 )); then
    printf 'virgl\n'
  elif (( venus_hit == 1 && virgl_hit == 1 )); then
    printf 'mixed\n'
  else
    printf 'unknown\n'
  fi
}

active_umd_contract_for_inf() {
  local inf="$1"
  local payload_names="$2"
  local dll_payload_names="$3"

  LC_ALL=C awk \
    -v expected_id="$(printf '%s' "$VIOGPU3D_PCI_DEVICE_ID" | tr '[:upper:]' '[:lower:]')" \
    -v payload_names="$payload_names" \
    -v dll_payload_names="$dll_payload_names" \
    '
      function trim(value) {
        sub(/^[[:space:]]+/, "", value)
        sub(/[[:space:]]+$/, "", value)
        return value
      }

      function strip_comment(value,    i, char, quoted, percent_token, result) {
        quoted = 0
        percent_token = 0
        result = ""
        for (i = 1; i <= length(value); i++) {
          char = substr(value, i, 1)
          if (char == "\"") {
            quoted = !quoted
          } else if (char == "%" && !quoted) {
            percent_token = !percent_token
          } else if (char == ";" && !quoted && !percent_token) {
            break
          }
          result = result char
        }
        return result
      }

      function clear_csv(    key) {
        for (key in csv_field) {
          delete csv_field[key]
        }
        for (key in csv_field_quoted) {
          delete csv_field_quoted[key]
        }
      }

      function parse_csv(value,    i, char, quoted, percent_token, count, field) {
        clear_csv()
        quoted = 0
        percent_token = 0
        count = 1
        field = ""
        for (i = 1; i <= length(value); i++) {
          char = substr(value, i, 1)
          if (char == "\"" && !percent_token) {
            csv_field_quoted[count] = 1
            quoted = !quoted
          } else if (char == "%" && !quoted) {
            if (substr(value, i + 1, 1) == "%") {
              field = field "%%"
              i++
            } else {
              percent_token = !percent_token
              field = field char
            }
          } else if (char == "," && !quoted && !percent_token) {
            csv_field[count++] = trim(field)
            field = ""
          } else {
            field = field char
          }
        }
        csv_field[count] = trim(field)
        if (quoted || percent_token) inf_syntax_invalid = 1
        return count
      }

      function unquoted_equals(value,    i, char, quoted) {
        quoted = 0
        for (i = 1; i <= length(value); i++) {
          char = substr(value, i, 1)
          if (char == "\"") quoted = !quoted
          else if (char == "=" && !quoted) return i
        }
        return 0
      }

      function normalize_lhs(value) {
        value = trim(value)
        if (length(value) >= 2 && substr(value, 1, 1) == "\"" && substr(value, length(value), 1) == "\"") {
          value = substr(value, 2, length(value) - 2)
        }
        return tolower(trim(value))
      }

      function strip_strings_comment(value,    i, char, quoted, equals_seen, result) {
        if (trim(value) ~ /^;/) return ""
        quoted = 0
        equals_seen = 0
        result = ""
        for (i = 1; i <= length(value); i++) {
          char = substr(value, i, 1)
          if (char == "\"") quoted = !quoted
          else if (char == "=" && !quoted) equals_seen = 1
          else if (char == ";" && !quoted && equals_seen) break
          result = result char
        }
        if (quoted) inf_syntax_invalid = 1
        return result
      }

      function record_string_definition(section_name, key, value,    localized) {
        key = tolower(trim(key))
        value = trim(value)
        if (key == "") {
          inf_syntax_invalid = 1
          return
        }
        if (substr(value, 1, 1) == "\"") {
          if (length(value) < 2 || substr(value, length(value), 1) != "\"") {
            inf_syntax_invalid = 1
            return
          }
          value = substr(value, 2, length(value) - 2)
        } else if (index(value, "\"") != 0) {
          inf_syntax_invalid = 1
          return
        }
        localized = section_name != "strings"
        if (localized) {
          localized_string_present[key] = 1
          return
        }
        if (string_present[key]) string_ambiguous[key] = 1
        string_present[key] = 1
        string_value[key] = value
      }

      function load_strings(    section_name, i, line, equals_at) {
        for (section_name in section_present) {
          if (section_name != "strings" && section_name !~ /^strings[.]/) continue
          for (i = 1; i <= section_line_count[section_name]; i++) {
            line = section_line[section_name SUBSEP i]
            equals_at = unquoted_equals(line)
            if (equals_at == 0) {
              inf_syntax_invalid = 1
              continue
            }
            record_string_definition(section_name, substr(line, 1, equals_at - 1), substr(line, equals_at + 1))
          }
        }
      }

      function source_disk_description_is_unsafe(value, quoted,    key, expanded) {
        if (quoted) return value == ""
        if (length(value) < 3 || substr(value, 1, 1) != "%" || substr(value, length(value), 1) != "%") return 1
        key = tolower(substr(value, 2, length(value) - 2))
        if (key == "" || index(key, "%") != 0) return 1
        if (!string_present[key] || string_ambiguous[key] || localized_string_present[key]) return 1
        expanded = string_value[key]
        return expanded == "" || expanded ~ /[,;\"%]/
      }

      function valid_disk_id(diskid) {
        return diskid ~ /^(0|[1-9][0-9]*)$/ && (diskid + 0) <= 4294967295
      }

      function valid_destination_dirid(dirid) {
        return dirid ~ /^[1-9][0-9]*$/ &&
          dirid != "65535" &&
          (dirid + 0) <= 4294967294
      }

      function record_source_disk_file(kind, source, diskid, subdir) {
        source = normalize_lhs(source)
        diskid = tolower(trim(diskid))
        subdir = trim(subdir)
        if (source == "" || !valid_disk_id(diskid)) {
          source_contract_invalid = 1
          return
        }
        if (kind == "arm64") {
          if (source_arm64_present[source]) source_arm64_ambiguous[source] = 1
          source_arm64_present[source] = 1
          source_arm64_diskid[source] = diskid
          source_arm64_subdir[source] = subdir
        } else {
          if (source_base_present[source]) source_base_ambiguous[source] = 1
          source_base_present[source] = 1
          source_base_diskid[source] = diskid
          source_base_subdir[source] = subdir
        }
      }

      function record_source_disk_name(kind, diskid, description, description_quoted, tag_or_cab, basepath, flags, tag_file,    nonflat) {
        diskid = normalize_lhs(diskid)
        tag_or_cab = trim(tag_or_cab)
        basepath = trim(basepath)
        flags = trim(flags)
        tag_file = trim(tag_file)
        nonflat = source_disk_description_is_unsafe(description, description_quoted) ||
          tag_or_cab != "" || basepath != "" || flags != "" || tag_file != ""
        if (!valid_disk_id(diskid)) {
          source_contract_invalid = 1
          return
        }
        if (kind == "arm64") {
          if (disk_name_arm64_present[diskid]) disk_name_arm64_ambiguous[diskid] = 1
          disk_name_arm64_present[diskid] = 1
          disk_name_arm64_nonflat[diskid] = nonflat
        } else {
          if (disk_name_base_present[diskid]) disk_name_base_ambiguous[diskid] = 1
          disk_name_base_present[diskid] = 1
          disk_name_base_nonflat[diskid] = nonflat
        }
      }

      function load_source_disk_files(source_section, kind,    i, line, equals_at, source, count, diskid, subdir) {
        for (i = 1; i <= section_line_count[source_section]; i++) {
          line = section_line[source_section SUBSEP i]
          equals_at = unquoted_equals(line)
          if (equals_at == 0) {
            source_contract_invalid = 1
            continue
          }
          source = substr(line, 1, equals_at - 1)
          count = parse_csv(substr(line, equals_at + 1))
          diskid = count >= 1 ? csv_field[1] : ""
          subdir = count >= 2 ? csv_field[2] : ""
          record_source_disk_file(kind, source, diskid, subdir)
        }
      }

      function load_source_disk_names(source_section, kind,    i, line, equals_at, diskid, count, description, description_quoted, tag_or_cab, basepath, flags, tag_file) {
        for (i = 1; i <= section_line_count[source_section]; i++) {
          line = section_line[source_section SUBSEP i]
          equals_at = unquoted_equals(line)
          if (equals_at == 0) {
            source_contract_invalid = 1
            continue
          }
          diskid = substr(line, 1, equals_at - 1)
          count = parse_csv(substr(line, equals_at + 1))
          description = count >= 1 ? csv_field[1] : ""
          description_quoted = csv_field_quoted[1] ? 1 : 0
          tag_or_cab = count >= 2 ? csv_field[2] : ""
          basepath = count >= 4 ? csv_field[4] : ""
          flags = count >= 5 ? csv_field[5] : ""
          tag_file = count >= 6 ? csv_field[6] : ""
          record_source_disk_name(kind, diskid, description, description_quoted, tag_or_cab, basepath, flags, tag_file)
        }
      }

      function load_source_disk_mappings(    source_section) {
        for (source_section in section_present) {
          if (source_section ~ /^sourcedisksfiles([.]|$)/ ||
              source_section ~ /^sourcedisksnames([.]|$)/) source_contract_present = 1
          if (source_section == "sourcedisksfiles") load_source_disk_files(source_section, "base")
          else if (source_section == "sourcedisksfiles.arm64") load_source_disk_files(source_section, "arm64")
          else if (source_section == "sourcedisksnames") load_source_disk_names(source_section, "base")
          else if (source_section == "sourcedisksnames.arm64") load_source_disk_names(source_section, "arm64")
        }
      }

      function source_disk_name_is_not_flat(diskid) {
        if (disk_name_arm64_ambiguous[diskid]) return 1
        if (disk_name_arm64_present[diskid]) return disk_name_arm64_nonflat[diskid]
        if (disk_name_base_ambiguous[diskid]) return 1
        if (disk_name_base_present[diskid]) return disk_name_base_nonflat[diskid]
        return 1
      }

      function source_disk_mapping_is_not_flat(source,    diskid, subdir) {
        source = tolower(trim(source))
        if (!source_contract_present) return 1
        if (source_contract_invalid) return 1
        if (source_arm64_ambiguous[source]) return 1
        if (source_arm64_present[source]) {
          diskid = source_arm64_diskid[source]
          subdir = source_arm64_subdir[source]
        } else {
          if (source_base_ambiguous[source] || !source_base_present[source]) return 1
          diskid = source_base_diskid[source]
          subdir = source_base_subdir[source]
        }
        if (subdir != "") return 1
        return source_disk_name_is_not_flat(diskid)
      }

      function clear_path_state(    key) {
        for (key in addreg_section) delete addreg_section[key]
        for (key in copy_section) delete copy_section[key]
        for (key in copy_source) delete copy_source[key]
        for (key in copy_target_present) delete copy_target_present[key]
        for (key in copy_dirid) delete copy_dirid[key]
        for (key in copy_subdir) delete copy_subdir[key]
        for (key in destination_dirid) delete destination_dirid[key]
        for (key in destination_subdir) delete destination_subdir[key]
        for (key in destination_present) delete destination_present[key]
        path_user_seen = 0
        path_open_gl_seen = 0
        path_display_seen = 0
        path_open_gl_version_seen = 0
        path_open_gl_flags_seen = 0
        path_copyfiles_payload_failed = 0
        path_registration_failed = 0
        path_direct_copy_count = 0
        path_section_copy_count = 0
      }

      function registered_dll_target(value, key,    count, part, target) {
        value = trim(value)
        gsub(/\\/, "/", value)
        count = split(value, part, "/")
        target = tolower(trim(part[count]))
        if (key == "installeddisplaydrivers" && target !~ /[.]dll$/) {
          target = target ".dll"
        }
        return target
      }

      function registered_dll_dirid(value, key,    normalized) {
        if (key == "installeddisplaydrivers") return "11"
        normalized = tolower(trim(value))
        gsub(/\\/, "/", normalized)
        return substr(normalized, 1, 5) == "%11%/" ? "11" : ""
      }

      function record_copy_mapping(target, source, dirid, subdir, flags) {
        target = tolower(trim(target))
        source = tolower(trim(source))
        dirid = tolower(trim(dirid))
        subdir = trim(subdir)
        flags = tolower(trim(flags))
        if (source == "") source = target
        if (target == "" ||
            substr(target, 1, 1) == "@" ||
            substr(source, 1, 1) == "@" ||
            index(target, "%") != 0 ||
            index(source, "%") != 0) {
          path_copyfiles_payload_failed = 1
          path_registration_failed = 1
        }
        if (flags != "" &&
            flags != "0" &&
            flags != "0x00000000" &&
            flags != "2" &&
            flags != "0x2" &&
            flags != "0x00000002") {
          path_copyfiles_payload_failed = 1
          path_registration_failed = 1
        }
        if (!valid_destination_dirid(dirid)) path_copyfiles_payload_failed = 1
        if (copy_target_present[target]) {
          path_copyfiles_payload_failed = 1
          path_registration_failed = 1
        } else {
          copy_target_present[target] = 1
          copy_source[target] = source
          copy_dirid[target] = dirid
          copy_subdir[target] = subdir
        }
        if (source == "" || !payload[source] || source_disk_mapping_is_not_flat(source)) {
          path_copyfiles_payload_failed = 1
        }
      }

      function select_arm64_install_section(base,    lower_base) {
        lower_base = tolower(trim(base))
        if (section_present[lower_base ".ntarm64"]) return lower_base ".ntarm64"
        if (section_present[lower_base ".nt"]) return lower_base ".nt"
        if (section_present[lower_base]) return lower_base
        return ""
      }

      function register_model_path(base, model,    key) {
        base = tolower(trim(base))
        model = tolower(trim(model))
        key = base SUBSEP model
        if (!model_path_seen[key]) {
          model_path_seen[key] = 1
          model_path_count[base]++
        }
        model_path_base[model] = base
        model_section[model] = 1
      }

      function hwid_component_present(value, component,    offset, relative, position, before, after) {
        value = tolower(value)
        component = tolower(component)
        offset = 1
        while (offset <= length(value)) {
          relative = index(substr(value, offset), component)
          if (relative == 0) return 0
          position = offset + relative - 1
          before = position == 1 ? "" : substr(value, position - 1, 1)
          after = substr(value, position + length(component), 1)
          if ((before == "" || before == "\\" || before == "&") &&
              (after == "" || after == "&")) return 1
          offset = position + 1
        }
        return 0
      }

      function model_hwid_matches(count,    i, value, wanted) {
        wanted = expected_id == "" ? "" : "dev_" expected_id
        for (i = 2; i <= count; i++) {
          value = tolower(csv_field[i])
          if (!hwid_component_present(value, "ven_1af4")) continue
          if (wanted != "") {
            if (hwid_component_present(value, wanted)) return 1
          } else if (hwid_component_present(value, "dev_1050") ||
                     hwid_component_present(value, "dev_10f7")) {
            return 1
          }
        }
        return 0
      }

      function evaluate_install_path(install, model,    i, line, equals_at, key, count, value, direct_source, section_name, target, source, dirid, subdir, copy_flags, registry_section, root, registry_subkey, registry_key, registry_flags, registry_kind, expected_target, expected_value, expected_data_count, data_count, data_index, valid) {
        clear_path_state()
        any_selected_install = 1
        if (first_selected_install == "") first_selected_install = install

        for (i = 1; i <= section_line_count["destinationdirs"]; i++) {
          line = section_line["destinationdirs" SUBSEP i]
          equals_at = index(line, "=")
          if (equals_at == 0) continue
          key = tolower(trim(substr(line, 1, equals_at - 1)))
          count = parse_csv(substr(line, equals_at + 1))
          if (key == "" || destination_present[key]) {
            path_copyfiles_payload_failed = 1
            path_registration_failed = 1
            continue
          }
          destination_present[key] = 1
          destination_dirid[key] = count >= 1 ? tolower(trim(csv_field[1])) : ""
          destination_subdir[key] = count >= 2 ? trim(csv_field[2]) : ""
        }

        for (i = 1; i <= section_line_count[install]; i++) {
          line = section_line[install SUBSEP i]
          equals_at = index(line, "=")
          if (equals_at == 0) continue
          key = tolower(trim(substr(line, 1, equals_at - 1)))
          count = parse_csv(substr(line, equals_at + 1))
          if (key == "needs") {
            for (data_index = 1; data_index <= count; data_index++) {
              if (trim(csv_field[data_index]) != "") {
                path_copyfiles_payload_failed = 1
                path_registration_failed = 1
              }
            }
          } else if (key == "addreg") {
            for (data_index = 1; data_index <= count; data_index++) {
              section_name = tolower(trim(csv_field[data_index]))
              if (section_name != "") addreg_section[section_name] = 1
            }
          } else if (key == "copyfiles") {
            for (data_index = 1; data_index <= count; data_index++) {
              value = trim(csv_field[data_index])
              if (substr(value, 1, 1) == "@") {
                path_direct_copy_count++
                direct_source = substr(value, 2)
                record_copy_mapping(direct_source, direct_source, destination_dirid["defaultdestdir"], destination_subdir["defaultdestdir"], "")
              } else if (value != "") {
                path_section_copy_count++
                section_name = tolower(value)
                if (copy_section[section_name]) {
                  path_copyfiles_payload_failed = 1
                  path_registration_failed = 1
                }
                copy_section[section_name] = 1
              }
            }
            if (path_direct_copy_count > 0 &&
                (path_direct_copy_count != 1 || path_section_copy_count != 0)) {
              path_copyfiles_payload_failed = 1
              path_registration_failed = 1
            }
          }
        }

        for (section_name in copy_section) {
          if (!section_present[section_name]) {
            path_copyfiles_payload_failed = 1
            continue
          }
          dirid = destination_dirid[section_name]
          subdir = destination_subdir[section_name]
          if (dirid == "") {
            dirid = destination_dirid["defaultdestdir"]
            subdir = destination_subdir["defaultdestdir"]
          }
          for (i = 1; i <= section_line_count[section_name]; i++) {
            line = section_line[section_name SUBSEP i]
            count = parse_csv(line)
            target = csv_field[1]
            source = count >= 2 ? csv_field[2] : ""
            copy_flags = count >= 4 ? csv_field[4] : ""
            record_copy_mapping(target, source, dirid, subdir, copy_flags)
          }
        }

        for (registry_section in addreg_section) {
          if (!section_present[registry_section]) {
            path_registration_failed = 1
            continue
          }
          for (i = 1; i <= section_line_count[registry_section]; i++) {
            line = section_line[registry_section SUBSEP i]
            count = parse_csv(line)
            if (count < 3) continue
            root = tolower(trim(csv_field[1]))
            registry_subkey = trim(csv_field[2])
            registry_key = tolower(trim(csv_field[3]))
            if (root != "hkr") continue
            if (registry_key == "usermodedrivername") {
              path_user_seen = 1
              registry_kind = "dll"
              expected_target = "viogpu_d3d10.dll"
              expected_data_count = 4
            } else if (registry_key == "opengldrivername") {
              path_open_gl_seen = 1
              registry_kind = "dll"
              expected_target = "viogpu_wgl.dll"
              expected_data_count = 1
            } else if (registry_key == "installeddisplaydrivers") {
              path_display_seen = 1
              registry_kind = "dll"
              expected_target = "viogpu_d3d10.dll"
              expected_data_count = 3
            } else if (registry_key == "openglversion") {
              path_open_gl_version_seen = 1
              registry_kind = "dword"
              expected_value = "4096"
              expected_data_count = 1
            } else if (registry_key == "openglflags") {
              path_open_gl_flags_seen = 1
              registry_kind = "dword"
              expected_value = "3"
              expected_data_count = 1
            } else {
              continue
            }

            registry_flags = count >= 4 ? tolower(trim(csv_field[4])) : ""
            if (registry_subkey != "") {
              path_registration_failed = 1
            }
            if (registry_kind == "dll" &&
                registry_flags != "0x00010000" &&
                registry_flags != "%reg_multi_sz%") path_registration_failed = 1
            if (registry_kind == "dword" &&
                registry_flags != "0x00010001" &&
                registry_flags != "%reg_dword%") path_registration_failed = 1
            data_count = 0
            for (data_index = 5; data_index <= count; data_index++) {
              value = trim(csv_field[data_index])
              if (value == "") continue
              data_count++
              if (registry_kind == "dll") {
                target = registered_dll_target(value, registry_key)
                source = copy_source[target]
                dirid = registered_dll_dirid(value, registry_key)
                if (target != expected_target ||
                    dirid != "11" ||
                    copy_dirid[target] != "11" ||
                    copy_subdir[target] != "" ||
                    source == "" ||
                    !payload[source] ||
                    !dll_payload[source]) path_registration_failed = 1
              } else if (value != expected_value) {
                path_registration_failed = 1
              }
            }
            if (data_count != expected_data_count) {
              path_registration_failed = 1
            }
          }
        }

        if (path_user_seen) any_user_seen = 1
        if (path_open_gl_seen) any_open_gl_seen = 1
        if (path_display_seen) any_display_seen = 1
        if (path_open_gl_version_seen) any_open_gl_version_seen = 1
        if (path_open_gl_flags_seen) any_open_gl_flags_seen = 1
        registration_valid = path_user_seen &&
          path_open_gl_seen &&
          path_display_seen &&
          path_open_gl_version_seen &&
          path_open_gl_flags_seen &&
          !path_registration_failed
        if (registration_valid) any_registered_dlls_resolved = 1

        valid = registration_valid && !path_copyfiles_payload_failed
        if (valid) {
          contract_valid = 1
          valid_model = model
          valid_install = install
        }
      }

      BEGIN {
        payload_count = split(payload_names, payload_item, "|")
        for (payload_index = 1; payload_index <= payload_count; payload_index++) {
          if (payload_item[payload_index] != "") {
            payload[tolower(payload_item[payload_index])] = 1
          }
        }
        dll_payload_count = split(dll_payload_names, dll_payload_item, "|")
        for (dll_payload_index = 1; dll_payload_index <= dll_payload_count; dll_payload_index++) {
          if (dll_payload_item[dll_payload_index] != "") {
            dll_payload[tolower(dll_payload_item[dll_payload_index])] = 1
          }
        }
        section = ""
      }

      {
        sub(/\r$/, "")
        if (section == "strings" || section ~ /^strings[.]/) {
          line = trim(strip_strings_comment($0))
        } else {
          line = trim(strip_comment($0))
        }
        if (line == "") next
        if (line ~ /^\[[^]]+\]$/) {
          section = tolower(trim(substr(line, 2, length(line) - 2)))
          section_present[section] = 1
          next
        }
        if (section != "") {
          section_line_count[section]++
          section_line[section SUBSEP section_line_count[section]] = line
        }
      }

      END {
        load_strings()
        load_source_disk_mappings()
        for (i = 1; i <= section_line_count["manufacturer"]; i++) {
          line = section_line["manufacturer" SUBSEP i]
          equals_at = index(line, "=")
          if (equals_at == 0) continue
          count = parse_csv(substr(line, equals_at + 1))
          model_base = tolower(trim(csv_field[1]))
          if (model_base == "") continue
          if (count == 1) register_model_path(model_base, model_base)
          for (field_index = 2; field_index <= count; field_index++) {
            decoration = tolower(trim(csv_field[field_index]))
            if (decoration == "ntarm64" || decoration ~ /^ntarm64[.]/) {
              register_model_path(model_base, model_base "." decoration)
            }
          }
        }

        for (model in model_section) {
          if (!section_present[model]) continue
          for (i = 1; i <= section_line_count[model]; i++) {
            line = section_line[model SUBSEP i]
            equals_at = index(line, "=")
            if (equals_at == 0) continue
            count = parse_csv(substr(line, equals_at + 1))
            if (count < 2 || !model_hwid_matches(count)) continue
            any_selected_model = 1
            selected_model_entry_count++
            selected_model_base[model_path_base[model]] = 1
            install = select_arm64_install_section(csv_field[1])
            if (install != "") evaluate_install_path(install, model)
          }
        }

        for (model_base in selected_model_base) {
          if (model_path_count[model_base] != 1) model_path_ambiguous = 1
        }
        if (selected_model_entry_count != 1) model_path_ambiguous = 1

        if (inf_syntax_invalid || model_path_ambiguous) {
          contract_valid = 0
          any_registered_dlls_resolved = 0
        }

        printf "contract_valid=%s\n", contract_valid ? "true" : "false"
        printf "user_mode_driver_name_registered=%s\n", any_user_seen ? "true" : "false"
        printf "open_gl_driver_name_registered=%s\n", any_open_gl_seen ? "true" : "false"
        printf "installed_display_drivers_registered=%s\n", any_display_seen ? "true" : "false"
        printf "open_gl_version_registered=%s\n", any_open_gl_version_seen ? "true" : "false"
        printf "open_gl_flags_registered=%s\n", any_open_gl_flags_seen ? "true" : "false"
        printf "registered_dlls_resolved=%s\n", any_registered_dlls_resolved ? "true" : "false"
        printf "active_copyfiles_payload_resolved=%s\n", contract_valid ? "true" : "false"
        printf "selected_model_found=%s\n", any_selected_model ? "true" : "false"
        printf "selected_install_found=%s\n", any_selected_install ? "true" : "false"
        printf "model_section=%s\n", contract_valid ? valid_model : "<none>"
        printf "install_section=%s\n", contract_valid ? valid_install : (first_selected_install == "" ? "<none>" : first_selected_install)
      }
    ' "$inf"
}

classify_render_capability() {
  local payload_names=""
  local dll_payload_names=""
  local file
  local inf
  local analysis
  local key
  local value
  local contract_valid=false
  local inf_contract_valid=false
  local inf_selected_model_found=false
  local inf_model_section="<none>"
  local inf_install_section="<none>"
  local selected_inf_count=0
  local valid_inf_count=0

  umd_user_mode_driver_name_registered=false
  umd_open_gl_driver_name_registered=false
  umd_installed_display_drivers_registered=false
  umd_open_gl_version_registered=false
  umd_open_gl_flags_registered=false
  umd_registered_dlls_resolved=false
  umd_active_copyfiles_payload_resolved=false
  umd_registration_inf="<none>"
  umd_registration_model_section="<none>"
  umd_registration_install_section="<none>"

  # CopyFiles may contain non-DLL runtime data (for example Vulkan ICD JSON),
  # so its source closure must be checked against every packaged file. DLL PE
  # validation remains the separate viogpu3d_dlls gate below.
  for file in "${viogpu3d_package_files[@]}"; do
    value="$(basename "$file" | tr '[:upper:]' '[:lower:]')"
    payload_names="${payload_names:+$payload_names|}$value"
  done
  if (( ${#viogpu3d_dlls[@]} > 0 )); then
    for file in "${viogpu3d_dlls[@]}"; do
      value="$(basename "$file" | tr '[:upper:]' '[:lower:]')"
      dll_payload_names="${dll_payload_names:+$dll_payload_names|}$value"
    done
    for inf in "${viogpu3d_infs[@]}"; do
      inf_contract_valid=false
      inf_selected_model_found=false
      inf_model_section="<none>"
      inf_install_section="<none>"
      analysis="$(active_umd_contract_for_inf "$inf" "$payload_names" "$dll_payload_names")"
      while IFS='=' read -r key value; do
        case "$key" in
          contract_valid)
            if [[ "$value" == "true" ]]; then inf_contract_valid=true; fi
            ;;
          user_mode_driver_name_registered)
            if [[ "$value" == "true" ]]; then umd_user_mode_driver_name_registered=true; fi
            ;;
          open_gl_driver_name_registered)
            if [[ "$value" == "true" ]]; then umd_open_gl_driver_name_registered=true; fi
            ;;
          installed_display_drivers_registered)
            if [[ "$value" == "true" ]]; then umd_installed_display_drivers_registered=true; fi
            ;;
          open_gl_version_registered)
            if [[ "$value" == "true" ]]; then umd_open_gl_version_registered=true; fi
            ;;
          open_gl_flags_registered)
            if [[ "$value" == "true" ]]; then umd_open_gl_flags_registered=true; fi
            ;;
          registered_dlls_resolved)
            if [[ "$value" == "true" ]]; then umd_registered_dlls_resolved=true; fi
            ;;
          active_copyfiles_payload_resolved)
            if [[ "$value" == "true" ]]; then umd_active_copyfiles_payload_resolved=true; fi
            ;;
          selected_model_found)
            if [[ "$value" == "true" ]]; then inf_selected_model_found=true; fi
            ;;
          model_section)
            if [[ "$value" != "<none>" ]]; then inf_model_section="$value"; fi
            ;;
          install_section)
            if [[ "$value" != "<none>" ]]; then inf_install_section="$value"; fi
            ;;
        esac
      done <<<"$analysis"
      if [[ "$inf_selected_model_found" == "true" ]]; then
        selected_inf_count=$((selected_inf_count + 1))
      fi
      if [[ "$inf_contract_valid" == "true" ]]; then
        valid_inf_count=$((valid_inf_count + 1))
        umd_registration_inf="$inf"
        umd_registration_model_section="$inf_model_section"
        umd_registration_install_section="$inf_install_section"
      fi
    done
    if (( selected_inf_count == 1 && valid_inf_count == 1 )); then
      contract_valid=true
      umd_user_mode_driver_name_registered=true
      umd_open_gl_driver_name_registered=true
      umd_installed_display_drivers_registered=true
      umd_open_gl_version_registered=true
      umd_open_gl_flags_registered=true
      umd_registered_dlls_resolved=true
      umd_active_copyfiles_payload_resolved=true
    elif (( selected_inf_count != 1 || valid_inf_count > 1 )); then
      contract_valid=false
      umd_registered_dlls_resolved=false
      umd_active_copyfiles_payload_resolved=false
      umd_registration_inf="<none>"
      umd_registration_model_section="<none>"
      umd_registration_install_section="<none>"
    fi
  fi

  if (( ${#viogpu3d_dlls[@]} == 0 )); then
    package_capability="kmd-only"
    umd_registration="absent"
    render_candidate=false
    render_candidate_reason="no-user-mode-dll-payload"
  elif [[ "$detected_protocol" != "virgl" ]]; then
    package_capability="umd-contract-unverified"
    umd_registration="protocol-specific"
    render_candidate=false
    render_candidate_reason="render-contract-not-defined-for-$detected_protocol"
  elif [[ "$contract_valid" == "true" &&
          "$umd_user_mode_driver_name_registered" == "true" &&
          "$umd_open_gl_driver_name_registered" == "true" &&
          "$umd_installed_display_drivers_registered" == "true" &&
          "$umd_open_gl_version_registered" == "true" &&
          "$umd_open_gl_flags_registered" == "true" &&
          "$umd_registered_dlls_resolved" == "true" &&
          "$umd_active_copyfiles_payload_resolved" == "true" ]]; then
    package_capability="umd-registered"
    umd_registration="complete"
    render_candidate=true
    render_candidate_reason="active-wddm-umd-registration-and-copyfiles-payload-present"
  elif [[ "$umd_user_mode_driver_name_registered" == "false" &&
          "$umd_open_gl_driver_name_registered" == "false" &&
          "$umd_installed_display_drivers_registered" == "false" &&
          "$umd_open_gl_version_registered" == "false" &&
          "$umd_open_gl_flags_registered" == "false" ]]; then
    package_capability="umd-payload-unregistered"
    umd_registration="absent"
    render_candidate=false
    render_candidate_reason="user-mode-dlls-present-but-inf-registration-missing"
  elif [[ "$umd_registered_dlls_resolved" == "true" &&
          "$umd_active_copyfiles_payload_resolved" != "true" ]]; then
    package_capability="umd-registration-active-payload-unresolved"
    umd_registration="complete-but-active-payload-unresolved"
    render_candidate=false
    render_candidate_reason="active-copyfiles-source-payload-unresolved"
  elif [[ "$umd_user_mode_driver_name_registered" == "true" &&
          "$umd_open_gl_driver_name_registered" == "true" &&
          "$umd_installed_display_drivers_registered" == "true" &&
          "$umd_open_gl_version_registered" == "true" &&
          "$umd_open_gl_flags_registered" == "true" ]]; then
    package_capability="umd-registration-dll-payload-unresolved"
    umd_registration="complete-but-unresolved"
    render_candidate=false
    render_candidate_reason="active-inf-registration-dll-payload-unresolved"
  else
    package_capability="umd-registration-incomplete"
    umd_registration="partial"
    render_candidate=false
    render_candidate_reason="required-inf-registration-incomplete"
  fi
}

sha256_for_file() {
  shasum -a 256 "$1" | awk '{ print $1 }'
}

size_for_file() {
  wc -c < "$1" | tr -d '[:space:]'
}

write_manifest_entry() {
  local role="$1"
  local path="$2"
  local machine="$3"
  printf 'file=%s\tsha256=%s\tsize=%s\tpe_machine=%s\tpath=%s\n' \
    "$role" "$(sha256_for_file "$path")" "$(size_for_file "$path")" "$machine" "$path"
}

write_manifest() {
  local manifest="$1"
  local tmp
  local file
  tmp="$manifest.tmp.$$"
  {
    printf 'BridgeVM viogpu3d package manifest\n'
    printf 'generated_utc=%s\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
    printf 'dir=%s\n' "$VIOGPU3D_DIR"
    printf 'provenance=%s\n' "$loaded_provenance"
    printf 'source_repo=%s\n' "$VIOGPU3D_SOURCE_REPO"
    printf 'source_ref=%s\n' "$VIOGPU3D_SOURCE_REF"
    printf 'build_id=%s\n' "$VIOGPU3D_BUILD_ID"
    printf 'signing_cert=%s\n' "$VIOGPU3D_SIGNING_CERT"
    printf 'hwids=%s\n' "$(join_hwids)"
    printf 'expected_hwid=%s\n' "${VIOGPU3D_PCI_DEVICE_ID:+PCI\\VEN_1AF4&DEV_$VIOGPU3D_PCI_DEVICE_ID}"
    printf 'protocol=%s\n' "$detected_protocol"
    printf 'protocol_source=%s\n' "$protocol_source"
    printf 'auto_protocol=%s\n' "$auto_protocol"
    printf 'package_capability=%s\n' "$package_capability"
    printf 'render_candidate=%s\n' "$render_candidate"
    printf 'render_candidate_reason=%s\n' "$render_candidate_reason"
    printf 'umd_registration=%s\n' "$umd_registration"
    printf 'umd_user_mode_driver_name_registered=%s\n' "$umd_user_mode_driver_name_registered"
    printf 'umd_open_gl_driver_name_registered=%s\n' "$umd_open_gl_driver_name_registered"
    printf 'umd_installed_display_drivers_registered=%s\n' "$umd_installed_display_drivers_registered"
    printf 'umd_open_gl_version_registered=%s\n' "$umd_open_gl_version_registered"
    printf 'umd_open_gl_flags_registered=%s\n' "$umd_open_gl_flags_registered"
    printf 'umd_registered_dlls_resolved=%s\n' "$umd_registered_dlls_resolved"
    printf 'umd_active_copyfiles_payload_resolved=%s\n' "$umd_active_copyfiles_payload_resolved"
    printf 'umd_registration_inf=%s\n' "$umd_registration_inf"
    printf 'umd_registration_model_section=%s\n' "$umd_registration_model_section"
    printf 'umd_registration_install_section=%s\n' "$umd_registration_install_section"
    printf 'inf_count=%s\n' "${#viogpu3d_infs[@]}"
    printf 'sys_count=%s\n' "${#viogpu3d_sys[@]}"
    printf 'cat_count=%s\n' "${#viogpu3d_cats[@]}"
    printf 'dll_count=%s\n' "${#viogpu3d_dlls[@]}"
    for file in "${viogpu3d_infs[@]}"; do
      write_manifest_entry inf "$file" n/a
    done
    for file in "${viogpu3d_sys[@]}"; do
      write_manifest_entry sys "$file" "$(pe_machine_hex "$file" "viogpu3d SYS")"
    done
    for file in "${viogpu3d_cats[@]}"; do
      write_manifest_entry cat "$file" n/a
    done
    if (( ${#viogpu3d_dlls[@]} > 0 )); then
      for file in "${viogpu3d_dlls[@]}"; do
        write_manifest_entry dll "$file" "$(pe_machine_hex "$file" "viogpu3d DLL")"
      done
    fi
  } > "$tmp"
  mv "$tmp" "$manifest"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --manifest)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      VIOGPU3D_MANIFEST="$2"
      shift 2
      ;;
    --provenance)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      VIOGPU3D_PROVENANCE="$2"
      VIOGPU3D_PROVENANCE_EXPLICIT=1
      shift 2
      ;;
    --pci-device-id)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      VIOGPU3D_PCI_DEVICE_ID="$2"
      shift 2
      ;;
    --require-render-candidate)
      VIOGPU3D_REQUIRE_RENDER_CANDIDATE="1"
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    -*)
      usage
      exit 2
      ;;
    *)
      [[ -z "$VIOGPU3D_DIR" || "$VIOGPU3D_DIR" == "$1" ]] || fail "multiple viogpu3d package directories specified"
      VIOGPU3D_DIR="$1"
      shift
      ;;
  esac
done

[[ -n "$VIOGPU3D_DIR" ]] || { usage; fail "VIOGPU3D_DIR is required"; }
[[ -d "$VIOGPU3D_DIR" ]] || fail "viogpu3d driver directory not found: $VIOGPU3D_DIR"
if [[ -z "$VIOGPU3D_PROVENANCE" ]]; then
  VIOGPU3D_PROVENANCE="$VIOGPU3D_DIR/bridgevm-package-provenance.env"
fi
load_provenance_defaults "$VIOGPU3D_PROVENANCE" "$VIOGPU3D_PROVENANCE_EXPLICIT"
case "$VIOGPU3D_PROTOCOL" in
  auto|venus|virgl) ;;
  *) fail "VIOGPU3D_PROTOCOL must be auto, venus, or virgl" ;;
esac
if [[ -n "$VIOGPU3D_PCI_DEVICE_ID" ]]; then
  VIOGPU3D_PCI_DEVICE_ID="$(normalize_pci_device_id "$VIOGPU3D_PCI_DEVICE_ID")"
fi
case "$VIOGPU3D_REQUIRE_RENDER_CANDIDATE" in
  0|1) ;;
  *) fail "VIOGPU3D_REQUIRE_RENDER_CANDIDATE must be 0 or 1" ;;
esac

reject_whitespace_path VIOGPU3D_DIR "$VIOGPU3D_DIR"

viogpu3d_infs=()
viogpu3d_sys=()
viogpu3d_cats=()
viogpu3d_dlls=()
viogpu3d_package_files=()
viogpu3d_hwids=""
for file in "$VIOGPU3D_DIR"/*; do
  [[ -f "$file" ]] || continue
  reject_windows_unsafe_basename "$file"
  viogpu3d_package_files+=("$file")
  extension="$(printf '%s\n' "${file##*.}" | tr '[:upper:]' '[:lower:]')"
  case "$extension" in
    inf) viogpu3d_infs+=("$file") ;;
    sys) viogpu3d_sys+=("$file") ;;
    cat) viogpu3d_cats+=("$file") ;;
    dll) viogpu3d_dlls+=("$file") ;;
  esac
done

(( ${#viogpu3d_infs[@]} > 0 )) || fail "no .inf found in $VIOGPU3D_DIR"
(( ${#viogpu3d_sys[@]} > 0 )) || fail "no .sys found in $VIOGPU3D_DIR"
(( ${#viogpu3d_cats[@]} > 0 )) || fail "no .cat catalog found in $VIOGPU3D_DIR"
detect_hwid_device_ids
[[ -n "$viogpu3d_hwids" ]] ||
  fail "viogpu3d INF/INX does not advertise a supported VirtIO GPU HWID: PCI\\VEN_1AF4&DEV_1050 or PCI\\VEN_1AF4&DEV_10F7"
if [[ -n "$VIOGPU3D_PCI_DEVICE_ID" ]]; then
  hwid_contains "$VIOGPU3D_PCI_DEVICE_ID" ||
    fail "viogpu3d INF/INX does not advertise expected PCI\\VEN_1AF4&DEV_$VIOGPU3D_PCI_DEVICE_ID; package hwids=$(join_hwids)"
fi

for sys in "${viogpu3d_sys[@]}"; do
  pe_arm64_machine_gate "$sys" "viogpu3d SYS"
done
if (( ${#viogpu3d_dlls[@]} > 0 )); then
  for dll in "${viogpu3d_dlls[@]}"; do
    pe_arm64_machine_gate "$dll" "viogpu3d DLL"
  done
fi

scan_files=("${viogpu3d_infs[@]}" "${viogpu3d_sys[@]}")
if (( ${#viogpu3d_dlls[@]} > 0 )); then
  scan_files+=("${viogpu3d_dlls[@]}")
fi
auto_protocol="$(detect_protocol)"
detected_protocol="$auto_protocol"
protocol_source="auto"
if [[ "$VIOGPU3D_PROTOCOL" != "auto" ]]; then
  if [[ "$auto_protocol" == "venus" || "$auto_protocol" == "virgl" ]]; then
    [[ "$auto_protocol" == "$VIOGPU3D_PROTOCOL" ]] || {
      fail "VIOGPU3D_PROTOCOL=$VIOGPU3D_PROTOCOL conflicts with package scan protocol=$auto_protocol"
    }
  fi
  detected_protocol="$VIOGPU3D_PROTOCOL"
  if [[ "$provenance_protocol_loaded" == "1" ]]; then
    protocol_source="provenance"
  else
    protocol_source="env"
  fi
fi
case "$detected_protocol" in
  venus|virgl) ;;
  mixed) fail "viogpu3d protocol is ambiguous; set VIOGPU3D_PROTOCOL=venus or virgl after package/source audit" ;;
  *) fail "could not identify viogpu3d protocol; set VIOGPU3D_PROTOCOL=venus or virgl after package/source audit" ;;
esac
classify_render_capability

printf 'BridgeVM viogpu3d package check\n'
printf 'dir=%s\n' "$VIOGPU3D_DIR"
printf 'inf_count=%s\n' "${#viogpu3d_infs[@]}"
printf 'sys_count=%s\n' "${#viogpu3d_sys[@]}"
printf 'cat_count=%s\n' "${#viogpu3d_cats[@]}"
printf 'dll_count=%s\n' "${#viogpu3d_dlls[@]}"
printf 'provenance=%s\n' "$loaded_provenance"
printf 'source_repo=%s\n' "$VIOGPU3D_SOURCE_REPO"
printf 'source_ref=%s\n' "$VIOGPU3D_SOURCE_REF"
printf 'build_id=%s\n' "$VIOGPU3D_BUILD_ID"
printf 'signing_cert=%s\n' "$VIOGPU3D_SIGNING_CERT"
printf 'hwids=%s\n' "$(join_hwids)"
printf 'expected_hwid=%s\n' "${VIOGPU3D_PCI_DEVICE_ID:+PCI\\VEN_1AF4&DEV_$VIOGPU3D_PCI_DEVICE_ID}"
printf 'protocol=%s\n' "$detected_protocol"
printf 'protocol_source=%s\n' "$protocol_source"
printf 'package_capability=%s\n' "$package_capability"
printf 'render_candidate=%s\n' "$render_candidate"
printf 'render_candidate_reason=%s\n' "$render_candidate_reason"
printf 'umd_registration=%s\n' "$umd_registration"
printf 'umd_user_mode_driver_name_registered=%s\n' "$umd_user_mode_driver_name_registered"
printf 'umd_open_gl_driver_name_registered=%s\n' "$umd_open_gl_driver_name_registered"
printf 'umd_installed_display_drivers_registered=%s\n' "$umd_installed_display_drivers_registered"
printf 'umd_open_gl_version_registered=%s\n' "$umd_open_gl_version_registered"
printf 'umd_open_gl_flags_registered=%s\n' "$umd_open_gl_flags_registered"
printf 'umd_registered_dlls_resolved=%s\n' "$umd_registered_dlls_resolved"
printf 'umd_active_copyfiles_payload_resolved=%s\n' "$umd_active_copyfiles_payload_resolved"
printf 'umd_registration_inf=%s\n' "$umd_registration_inf"
printf 'umd_registration_model_section=%s\n' "$umd_registration_model_section"
printf 'umd_registration_install_section=%s\n' "$umd_registration_install_section"
if [[ -n "$VIOGPU3D_MANIFEST" ]]; then
  write_manifest "$VIOGPU3D_MANIFEST"
  printf 'manifest=%s\n' "$VIOGPU3D_MANIFEST"
fi
if (( ${#viogpu3d_dlls[@]} == 0 )); then
  printf 'warning=no viogpu3d .dll files found; package appears KMD-only\n'
elif [[ "$render_candidate" != "true" ]]; then
  printf 'warning=user-mode DLL payload is present but required INF UMD registration or active CopyFiles payload closure is incomplete; package is not a render candidate\n'
fi
if [[ "$VIOGPU3D_REQUIRE_RENDER_CANDIDATE" == "1" && "$render_candidate" != "true" ]]; then
  fail "viogpu3d package is injection-ready but not a render candidate: $render_candidate_reason"
fi
printf 'PASS: viogpu3d package is injection-ready\n'
