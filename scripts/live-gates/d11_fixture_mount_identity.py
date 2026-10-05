"""Bind ATTACH's APFS type to the same current owned INFO mount and device."""
import plistlib
import re

from d11_fixture_files import read


def device(image):
    candidates = [row.get("dev-entry", "") for row in image.get("system-entities", [])
                  if row.get("content-hint") == "GUID_partition_scheme"]
    if not candidates:
        candidates = [row.get("dev-entry", "") for row in image.get("system-entities", [])
                      if re.fullmatch(r"/dev/disk[0-9]+", row.get("dev-entry", ""))]
    if len(candidates) != 1 or not re.fullmatch(r"/dev/disk[0-9]+", candidates[0]):
        raise ValueError("owned whole-disk identity unproved")
    return candidates[0]


def entities(value):
    if (type(value) is not dict or type(value.get("system-entities")) is not list
            or not 0 < len(value["system-entities"]) <= 128
            or any(type(row) is not dict for row in value["system-entities"])):
        raise ValueError("owned mount metadata malformed")
    return value["system-entities"]


def validate_owned_mount(attach_path, image, mount, backing):
    attached = plistlib.loads(read(attach_path, 65536))
    attach_rows, current_rows = entities(attached), entities(image)
    if image.get("image-path") != str(backing):
        raise ValueError("current mount backing differs")
    attach_matches = [row for row in attach_rows if row.get("mount-point") == str(mount)]
    current_matches = [row for row in current_rows if row.get("mount-point") == str(mount)]
    if len(attach_matches) != 1 or len(current_matches) != 1:
        raise ValueError("owned mount metadata ambiguous")
    attachment, current = attach_matches[0], current_matches[0]
    leaf = attachment.get("dev-entry")
    if (attachment.get("volume-kind") != "apfs" or type(leaf) is not str
            or not re.fullmatch(r"/dev/disk[0-9]+s[0-9]+", leaf)
            or current.get("dev-entry") != leaf
            or ("volume-kind" in current and current["volume-kind"] != "apfs")):
        raise ValueError("owned APFS mount identity differs")
    if (sum(row.get("dev-entry") == leaf for row in attach_rows) != 1
            or sum(row.get("dev-entry") == leaf for row in current_rows) != 1
            or device(attached) != device(image)):
        raise ValueError("owned mount device association differs")
