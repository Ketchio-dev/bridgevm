"""Regression for actual HVF pflash media, not the independent-board seed."""


def assert_vars_sizes(case, records):
    import winpe_companion_inputs as inputs

    with case.assertRaises(ValueError):
        inputs.verify(dict(records, injector=records["image"]))
    path = records["vars"][0].with_name("size-case.fd")
    for size in (0, 65536, 64 * 1024 * 1024 + 1, 64 * 1024 * 1024):
        with case.subTest(vars_bytes=size):
            with path.open("wb") as stream:
                stream.truncate(size)
            candidate = dict(records, vars=(path, inputs.file_hash(path)))
            if size == 64 * 1024 * 1024:
                inputs.verify(candidate)
            else:
                with case.assertRaisesRegex(ValueError, "invalid vars size"):
                    inputs.verify(candidate)
