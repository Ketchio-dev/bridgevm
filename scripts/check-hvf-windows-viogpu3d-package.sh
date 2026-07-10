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
                     with the registered DLLs actively copied to DirID 11.
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
  local match
  local id
  viogpu3d_hwids=""
  for inf in "${viogpu3d_infs[@]}"; do
    while IFS= read -r match; do
      [[ -n "$match" ]] || continue
      id="$(printf '%s\n' "$match" | grep -Eio 'DEV_[0-9A-F]{4}' | head -n 1 | cut -d_ -f2 | tr '[:lower:]' '[:upper:]')"
      case "$id" in
        1050|10F7) append_unique_hwid "$id" ;;
      esac
    done < <(
      LC_ALL=C grep -Eio 'VEN_1AF4[^[:space:],;]*DEV_(1050|10F7)|DEV_(1050|10F7)[^[:space:],;]*VEN_1AF4' "$inf" 2>/dev/null || true
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

  LC_ALL=C awk \
    -v expected_id="$(printf '%s' "$VIOGPU3D_PCI_DEVICE_ID" | tr '[:upper:]' '[:lower:]')" \
    -v payload_names="$payload_names" \
    '
      function trim(value) {
        sub(/^[[:space:]]+/, "", value)
        sub(/[[:space:]]+$/, "", value)
        return value
      }

      function strip_comment(value,    i, char, quoted, result) {
        quoted = 0
        result = ""
        for (i = 1; i <= length(value); i++) {
          char = substr(value, i, 1)
          if (char == "\"") {
            quoted = !quoted
          } else if (char == ";" && !quoted) {
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
      }

      function parse_csv(value,    i, char, quoted, count, field) {
        clear_csv()
        quoted = 0
        count = 1
        field = ""
        for (i = 1; i <= length(value); i++) {
          char = substr(value, i, 1)
          if (char == "\"") {
            quoted = !quoted
          } else if (char == "," && !quoted) {
            csv_field[count++] = trim(field)
            field = ""
          } else {
            field = field char
          }
        }
        csv_field[count] = trim(field)
        return count
      }

      function clear_path_state(    key) {
        for (key in addreg_section) delete addreg_section[key]
        for (key in copy_section) delete copy_section[key]
        for (key in copy_source) delete copy_source[key]
        for (key in copy_dirid) delete copy_dirid[key]
        for (key in copy_subdir) delete copy_subdir[key]
        for (key in destination_dirid) delete destination_dirid[key]
        for (key in destination_subdir) delete destination_subdir[key]
        path_user_seen = 0
        path_open_gl_seen = 0
        path_display_seen = 0
        path_open_gl_version_seen = 0
        path_open_gl_flags_seen = 0
        path_resolution_failed = 0
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

      function record_copy_mapping(target, source, dirid, subdir,    old_source, old_dirid, old_subdir) {
        target = tolower(trim(target))
        source = tolower(trim(source))
        dirid = tolower(trim(dirid))
        subdir = trim(subdir)
        sub(/^@/, "", target)
        sub(/^@/, "", source)
        if (source == "") source = target
        if (target ~ /[.]dll$/ && source ~ /[.]dll$/) {
          old_source = copy_source[target]
          old_dirid = copy_dirid[target]
          old_subdir = copy_subdir[target]
          if (old_source != "" &&
              (old_source != source || old_dirid != dirid || old_subdir != subdir)) {
            path_resolution_failed = 1
          }
          copy_source[target] = source
          copy_dirid[target] = dirid
          copy_subdir[target] = subdir
        }
      }

      function select_arm64_install_section(base,    lower_base) {
        lower_base = tolower(trim(base))
        if (section_present[lower_base ".ntarm64"]) return lower_base ".ntarm64"
        if (section_present[lower_base ".nt"]) return lower_base ".nt"
        if (section_present[lower_base]) return lower_base
        return ""
      }

      function model_hwid_matches(count,    i, value, wanted) {
        wanted = expected_id == "" ? "" : "dev_" expected_id
        for (i = 2; i <= count; i++) {
          value = tolower(csv_field[i])
          if (index(value, "ven_1af4") == 0) continue
          if (wanted != "") {
            if (index(value, wanted) != 0) return 1
          } else if (index(value, "dev_1050") != 0 || index(value, "dev_10f7") != 0) {
            return 1
          }
        }
        return 0
      }

      function evaluate_install_path(install, model,    i, line, equals_at, key, count, value, section_name, target, source, dirid, subdir, registry_section, root, registry_subkey, registry_key, registry_flags, registry_kind, expected_target, expected_value, expected_data_count, data_count, data_index, valid) {
        clear_path_state()
        any_selected_install = 1
        if (first_selected_install == "") first_selected_install = install

        for (i = 1; i <= section_line_count["destinationdirs"]; i++) {
          line = section_line["destinationdirs" SUBSEP i]
          equals_at = index(line, "=")
          if (equals_at == 0) continue
          key = tolower(trim(substr(line, 1, equals_at - 1)))
          count = parse_csv(substr(line, equals_at + 1))
          destination_dirid[key] = count >= 1 ? tolower(trim(csv_field[1])) : ""
          destination_subdir[key] = count >= 2 ? trim(csv_field[2]) : ""
        }

        for (i = 1; i <= section_line_count[install]; i++) {
          line = section_line[install SUBSEP i]
          equals_at = index(line, "=")
          if (equals_at == 0) continue
          key = tolower(trim(substr(line, 1, equals_at - 1)))
          count = parse_csv(substr(line, equals_at + 1))
          if (key == "addreg") {
            for (data_index = 1; data_index <= count; data_index++) {
              section_name = tolower(trim(csv_field[data_index]))
              if (section_name != "") addreg_section[section_name] = 1
            }
          } else if (key == "copyfiles") {
            for (data_index = 1; data_index <= count; data_index++) {
              value = trim(csv_field[data_index])
              if (substr(value, 1, 1) == "@") {
                record_copy_mapping(value, value, destination_dirid["defaultdestdir"], destination_subdir["defaultdestdir"])
              } else if (value != "") {
                copy_section[tolower(value)] = 1
              }
            }
          }
        }

        for (section_name in copy_section) {
          if (!section_present[section_name]) {
            path_resolution_failed = 1
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
            record_copy_mapping(target, source, dirid, subdir)
          }
        }

        for (registry_section in addreg_section) {
          if (!section_present[registry_section]) {
            path_resolution_failed = 1
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
              path_resolution_failed = 1
            }
            if (registry_kind == "dll" &&
                registry_flags != "0x00010000" &&
                registry_flags != "%reg_multi_sz%") path_resolution_failed = 1
            if (registry_kind == "dword" &&
                registry_flags != "0x00010001" &&
                registry_flags != "%reg_dword%") path_resolution_failed = 1
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
                    !payload[source]) path_resolution_failed = 1
              } else if (value != expected_value) {
                path_resolution_failed = 1
              }
            }
            if (data_count != expected_data_count) {
              path_resolution_failed = 1
            }
          }
        }

        if (path_user_seen) any_user_seen = 1
        if (path_open_gl_seen) any_open_gl_seen = 1
        if (path_display_seen) any_display_seen = 1
        if (path_open_gl_version_seen) any_open_gl_version_seen = 1
        if (path_open_gl_flags_seen) any_open_gl_flags_seen = 1
        if (path_resolution_failed) any_resolution_failed = 1

        valid = path_user_seen &&
          path_open_gl_seen &&
          path_display_seen &&
          path_open_gl_version_seen &&
          path_open_gl_flags_seen &&
          !path_resolution_failed
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
        section = ""
      }

      {
        sub(/\r$/, "")
        line = trim(strip_comment($0))
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
        for (i = 1; i <= section_line_count["manufacturer"]; i++) {
          line = section_line["manufacturer" SUBSEP i]
          equals_at = index(line, "=")
          if (equals_at == 0) continue
          count = parse_csv(substr(line, equals_at + 1))
          model_base = tolower(trim(csv_field[1]))
          if (model_base == "") continue
          if (count == 1) model_section[model_base] = 1
          for (field_index = 2; field_index <= count; field_index++) {
            decoration = tolower(trim(csv_field[field_index]))
            if (decoration == "ntarm64" || decoration ~ /^ntarm64[.]/) {
              model_section[model_base "." decoration] = 1
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
            install = select_arm64_install_section(csv_field[1])
            if (install != "") evaluate_install_path(install, model)
          }
        }

        printf "contract_valid=%s\n", contract_valid ? "true" : "false"
        printf "user_mode_driver_name_registered=%s\n", any_user_seen ? "true" : "false"
        printf "open_gl_driver_name_registered=%s\n", any_open_gl_seen ? "true" : "false"
        printf "installed_display_drivers_registered=%s\n", any_display_seen ? "true" : "false"
        printf "open_gl_version_registered=%s\n", any_open_gl_version_seen ? "true" : "false"
        printf "open_gl_flags_registered=%s\n", any_open_gl_flags_seen ? "true" : "false"
        printf "registered_dlls_resolved=%s\n", contract_valid ? "true" : "false"
        printf "selected_model_found=%s\n", any_selected_model ? "true" : "false"
        printf "selected_install_found=%s\n", any_selected_install ? "true" : "false"
        printf "model_section=%s\n", contract_valid ? valid_model : "<none>"
        printf "install_section=%s\n", contract_valid ? valid_install : (first_selected_install == "" ? "<none>" : first_selected_install)
      }
    ' "$inf"
}

classify_render_capability() {
  local payload_names=""
  local dll
  local inf
  local analysis
  local key
  local value
  local contract_valid=false

  umd_user_mode_driver_name_registered=false
  umd_open_gl_driver_name_registered=false
  umd_installed_display_drivers_registered=false
  umd_open_gl_version_registered=false
  umd_open_gl_flags_registered=false
  umd_registered_dlls_resolved=false
  umd_registration_inf="<none>"
  umd_registration_model_section="<none>"
  umd_registration_install_section="<none>"

  if (( ${#viogpu3d_dlls[@]} > 0 )); then
    for dll in "${viogpu3d_dlls[@]}"; do
      value="$(basename "$dll" | tr '[:upper:]' '[:lower:]')"
      payload_names="${payload_names:+$payload_names|}$value"
    done
    for inf in "${viogpu3d_infs[@]}"; do
      analysis="$(active_umd_contract_for_inf "$inf" "$payload_names")"
      while IFS='=' read -r key value; do
        case "$key" in
          contract_valid)
            if [[ "$value" == "true" ]]; then contract_valid=true; fi
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
          model_section)
            if [[ "$value" != "<none>" ]]; then umd_registration_model_section="$value"; fi
            ;;
          install_section)
            if [[ "$value" != "<none>" ]]; then umd_registration_install_section="$value"; fi
            ;;
        esac
      done <<<"$analysis"
      if [[ "$contract_valid" == "true" ]]; then
        umd_registration_inf="$inf"
        umd_user_mode_driver_name_registered=true
        umd_open_gl_driver_name_registered=true
        umd_installed_display_drivers_registered=true
        umd_open_gl_version_registered=true
        umd_open_gl_flags_registered=true
        umd_registered_dlls_resolved=true
        break
      fi
    done
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
          "$umd_registered_dlls_resolved" == "true" ]]; then
    package_capability="umd-registered"
    umd_registration="complete"
    render_candidate=true
    render_candidate_reason="active-wddm-umd-registration-and-packaged-dlls-present"
  elif [[ "$umd_user_mode_driver_name_registered" == "false" &&
          "$umd_open_gl_driver_name_registered" == "false" &&
          "$umd_installed_display_drivers_registered" == "false" &&
          "$umd_open_gl_version_registered" == "false" &&
          "$umd_open_gl_flags_registered" == "false" ]]; then
    package_capability="umd-payload-unregistered"
    umd_registration="absent"
    render_candidate=false
    render_candidate_reason="user-mode-dlls-present-but-inf-registration-missing"
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
viogpu3d_hwids=""
shopt -s nullglob
for file in "$VIOGPU3D_DIR"/*.inf "$VIOGPU3D_DIR"/*.INF; do viogpu3d_infs+=("$file"); done
for file in "$VIOGPU3D_DIR"/*.sys "$VIOGPU3D_DIR"/*.SYS; do viogpu3d_sys+=("$file"); done
for file in "$VIOGPU3D_DIR"/*.cat "$VIOGPU3D_DIR"/*.CAT; do viogpu3d_cats+=("$file"); done
for file in "$VIOGPU3D_DIR"/*.dll "$VIOGPU3D_DIR"/*.DLL; do viogpu3d_dlls+=("$file"); done
shopt -u nullglob

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
  printf 'warning=user-mode DLL payload is present but required INF UMD registration is incomplete; package is not a render candidate\n'
fi
if [[ "$VIOGPU3D_REQUIRE_RENDER_CANDIDATE" == "1" && "$render_candidate" != "true" ]]; then
  fail "viogpu3d package is injection-ready but not a render candidate: $render_candidate_reason"
fi
printf 'PASS: viogpu3d package is injection-ready\n'
