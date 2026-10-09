"""Phase3 source binding regressions shared by the production T22 contracts."""
from pathlib import Path
import json
import shutil
import tempfile
from unittest.mock import patch

import a19_interrupt_auxiliary as auxiliary
import a19_interrupt_auxiliary_io as io
from a19_auxiliary_protocol import selected as retained_pair
from a19_collection_fixtures import lines, manifest, put, selected
from a19_interrupt_stop_points import CREATE_VM_ID


class AuxiliarySeedBinding:
    def test_self_consistent_seed_with_only_wrong_vars_refuses_before_restore(self):
        args, before = self.run_case("seed-wrong-vars")
        source, authenticated = io.pair(args[2], args[3]), []
        def verify(path):
            value = io.manifest(path)
            if path.name == "seed.snapshot":
                authenticated.append(value)
            return value
        with patch.object(auxiliary, "manifest", side_effect=verify):
            with self.assertRaisesRegex(ValueError, "seed creation"):
                auxiliary.run(*args, deadline=5, budget=60, clone=shutil.copyfile)
        seed = io.parsed((args[-1] / "aux-swap/seed-create.stdout").read_bytes(), io.MANIFEST_FIELDS)
        self.assertEqual(authenticated, [seed])  # Real seed media and manifest agreed before cleanup.
        self.assertEqual(seed["disk_sha256"], source["disk_sha256"])
        self.assertEqual(seed["vars_bytes"], source["vars_bytes"])
        self.assertNotEqual(seed["vars_sha256"], source["vars_sha256"])
        calls = json.loads((args[0].parent / "calls.json").read_text())
        self.assertEqual([call[0] for call in calls], ["create"])
        self.assertEqual([args[2].read_bytes(), args[3].read_bytes()], before)
        self.assertFalse((args[-1] / "live/auxiliary").exists())

    def test_nondistinct_source_refuses_before_clone_or_helper_launch(self):
        args, _ = self.run_case()
        args[2].write_bytes((args[1] / "disk.raw").read_bytes())
        with self.assertRaisesRegex(ValueError, "distinct"):
            auxiliary.run(*args, deadline=1, budget=30, clone=shutil.copyfile)
        self.assertFalse((args[0].parent / "calls.json").exists())
        self.assertFalse((args[-1] / "live/auxiliary").exists())


class ProductionSourceBinding:
    def test_coordinated_wrong_old_pair_refuses_even_when_all_host_proofs_agree(self):
        for member in ("disk", "vars"):
            for cases in (("swap",), ("create",), ("swap", "create")):
                with self.subTest(member=member, cases=cases), tempfile.TemporaryDirectory() as temporary:
                    output = Path(temporary)
                    value = self.prepared(output)
                    wrong = retained_pair(output / "aux-swap", "preinterrupt")
                    wrong[member + "_sha256"] = "f" * 64
                    for case in cases:
                        root = output / f"aux-{case}"
                        names = (("preinterrupt", "postkill") if case == "swap" else
                                 ("source", "source-postkill", "source-postretry", "postretry"))
                        for name in names:
                            selected(root, name, wrong)
                        if case == "create":
                            put(root, "retry.stdout", lines(manifest(wrong, CREATE_VM_ID)))
                            put(root, "retry-manifest.json", json.dumps(manifest(wrong, CREATE_VM_ID)))
                    self.refused(output, value)

    def test_swap_retry_with_only_one_restored_member_refuses(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            value = self.prepared(output)
            root = output / "aux-swap"
            old_vars = (root / "preinterrupt-digest.txt").read_text().splitlines()[3]
            for suffix in ("-digest.txt", "-host-digest.txt"):
                path = root / ("postretry" + suffix)
                lines = path.read_text().splitlines()
                lines[3] = old_vars
                path.write_text("\n".join(lines) + "\n")
            self.refused(output, value)
