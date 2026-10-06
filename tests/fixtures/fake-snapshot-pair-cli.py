#!/usr/bin/env python3
"""Fixture snapshot_pair_cli for lanes without a managed generation."""
import sys
sys.dont_write_bytecode = True
from fake_snapshot_pair_commands import command

sys.exit(command(sys.argv[1:]))
