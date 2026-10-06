"""Package the deterministic pair helper as an app asset before sealing fixtures."""
from pathlib import Path
import shutil


def package_pair_helper(app: Path, repository: Path) -> None:
    directory = app / "Contents/Resources/target/release/examples"
    directory.mkdir(parents=True)
    fixtures = repository / "tests/fixtures"
    shutil.copyfile(fixtures / "fake-snapshot-pair-cli.py", directory / "snapshot_pair_cli")
    shutil.copyfile(fixtures / "fake_snapshot_pair_commands.py", directory / "fake_snapshot_pair_commands.py")
    (directory / "snapshot_pair_cli").chmod(0o700)
