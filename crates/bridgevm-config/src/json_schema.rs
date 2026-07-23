//! The published JSON Schema document for the manifest and its $id.

pub const MANIFEST_JSON_SCHEMA_ID: &str = "https://bridgevm.io/schemas/vm-manifest-v1.schema.json";

pub fn manifest_json_schema_v1() -> &'static str {
    r#"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://bridgevm.io/schemas/vm-manifest-v1.schema.json",
  "title": "BridgeVM VM Manifest",
  "type": "object",
  "additionalProperties": false,
  "required": [
    "schemaVersion",
    "name",
    "mode",
    "guest",
    "backend",
    "resources",
    "display",
    "storage",
    "network",
    "integration",
    "security"
  ],
  "properties": {
    "schemaVersion": {
      "const": "bridgevm.io/v1"
    },
    "name": {
      "type": "string",
      "minLength": 1
    },
    "mode": {
      "enum": ["fast", "compatibility"]
    },
    "guest": {
      "type": "object",
      "additionalProperties": false,
      "required": ["os", "arch"],
      "properties": {
        "os": { "type": "string", "minLength": 1 },
        "version": { "type": "string" },
        "arch": { "type": "string", "minLength": 1 }
      }
    },
    "backend": {
      "type": "object",
      "additionalProperties": false,
      "required": ["engine"],
      "properties": {
        "engine": { "type": "string", "minLength": 1 },
        "preferred": { "type": "string" },
        "fallback": { "type": "string" },
        "accelerator": { "type": "string" }
      }
    },
    "resources": {
      "type": "object",
      "additionalProperties": false,
      "required": ["profile", "memory", "cpu"],
      "properties": {
        "profile": { "type": "string", "minLength": 1 },
        "memory": { "type": "string", "minLength": 1 },
        "cpu": { "type": "string", "minLength": 1 }
      }
    },
    "display": {
      "type": "object",
      "additionalProperties": false,
      "required": ["renderer", "framePolicy", "retina"],
      "properties": {
        "renderer": { "type": "string", "minLength": 1 },
        "framePolicy": { "type": "string", "minLength": 1 },
        "retina": { "type": "boolean" }
      }
    },
    "storage": {
      "type": "object",
      "additionalProperties": false,
      "required": ["primary"],
      "properties": {
        "primary": {
          "type": "object",
          "additionalProperties": false,
          "required": ["path", "size", "format", "discard"],
          "properties": {
            "path": { "type": "string", "minLength": 1 },
            "size": { "type": "string", "minLength": 1 },
            "format": { "type": "string", "minLength": 1 },
            "discard": { "type": "boolean" }
          }
        }
      }
    },
    "boot": {
      "type": "object",
      "additionalProperties": false,
      "required": ["mode"],
      "properties": {
        "mode": {
          "enum": ["existing-disk", "linux-kernel", "linux-installer", "windows-installer", "macos-restore"]
        },
        "installerImage": { "type": "string" },
        "kernelPath": { "type": "string" },
        "initrdPath": { "type": "string" },
        "kernelCommandLine": { "type": "string" },
        "macosRestoreImage": { "type": "string" }
      }
    },
    "network": {
      "type": "object",
      "additionalProperties": false,
      "required": ["mode", "hostname"],
      "properties": {
        "mode": { "type": "string", "minLength": 1 },
        "hostname": { "type": "string", "minLength": 1 },
        "bridgeInterface": { "type": "string", "minLength": 1 },
        "forwards": {
          "type": "array",
          "items": {
            "type": "object",
            "additionalProperties": false,
            "required": ["host", "guest"],
            "properties": {
              "host": { "type": "integer", "minimum": 1, "maximum": 65535 },
              "guest": { "type": "integer", "minimum": 1, "maximum": 65535 }
            }
          }
        }
      }
    },
    "integration": {
      "type": "object",
      "additionalProperties": false,
      "required": ["tools", "clipboard", "dragDrop", "dynamicResolution", "sharedFolders"],
      "properties": {
        "tools": { "type": "string", "minLength": 1 },
        "clipboard": { "type": "boolean" },
        "dragDrop": { "type": "boolean" },
        "dynamicResolution": { "type": "boolean" },
        "sharedFolders": { "type": "boolean" },
        "applications": { "type": "boolean" },
        "windows": { "type": "boolean" }
      }
    },
    "security": {
      "type": "object",
      "additionalProperties": false,
      "required": ["sharedFolderApproval", "guestCommandExecution", "signedAgentUpdates"],
      "properties": {
        "sharedFolderApproval": { "type": "string", "minLength": 1 },
        "guestCommandExecution": { "type": "boolean" },
        "signedAgentUpdates": { "type": "boolean" }
      }
    },
    "firmware": {
      "type": "object",
      "additionalProperties": false,
      "properties": {
        "nvmeTarget": { "type": "boolean" },
        "tpm": { "type": "boolean" },
        "secureBoot": { "type": "boolean" }
      }
    },
    "sharedFolders": {
      "type": "array",
      "items": {
        "type": "object",
        "additionalProperties": false,
        "required": ["name", "hostPath"],
        "properties": {
          "name": { "type": "string", "minLength": 1 },
          "hostPath": { "type": "string", "minLength": 1 },
          "readOnly": { "type": "boolean" },
          "hostPathToken": { "type": "string", "minLength": 1 }
        }
      }
    }
  }
}
"#
}
