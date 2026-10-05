"""Admit sealed native helpers and the runner's explicit renderer dependency."""
from d11_fixture_files import read


def dependencies(raw):
    lines = raw.decode("utf-8").splitlines()
    if not lines or len(lines) > 128: raise ValueError("native linkage unavailable")
    values = []
    for line in lines[1:]:
        path, separator, _ = line.strip().partition(" (compatibility version ")
        if not separator or not path.startswith("/"):
            raise ValueError("nonabsolute native dependency")
        values.append(path)
    if not values: raise ValueError("empty native linkage")
    return values


def system(path): return path.startswith(("/usr/lib/", "/System/Library/"))


def admit(inputs, probe, output, processes):
    tools = inputs.path("tools")
    helper = inputs.path("fixture_helper") / "d11-fixture-helper"
    for index, path in enumerate((helper, tools / "wimlib-imagex", tools / "bridgevm-catalog-verify", probe)):
        log = output / f"linkage-{index}.private.log"
        processes.run(["/usr/bin/otool", "-L", str(path)], log)
        links = dependencies(read(log))
        if path == probe:
            renderer = str(inputs.path("renderer"))
            if renderer not in links or any(not system(p) and p != renderer for p in links):
                raise ValueError("runner renderer or native dependency differs")
        elif any(not system(p) for p in links):
            raise ValueError("helper has an unsealed native dependency")
    processes.run(["/usr/bin/codesign", "--verify", "--strict", str(probe)], output / "codesign.private.log")
    inputs.check()
