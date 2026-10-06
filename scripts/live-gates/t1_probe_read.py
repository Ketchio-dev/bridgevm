"""Serve bound legacy T1 evidence without bypassing a conflicting queue tier."""
import json
import sys
from t1_probe_receipt import load, validate


def serve_t1(directory, tiers, commit, job_id):
    if any(tier not in (None, 't1-vtimer') for tier in tiers):raise ValueError('T1 queue tier differs')
    value = validate(directory, load(directory / 'receipt.public.json'), commit, job_id, require_default=True)
    if value != load(directory / 'receipt.json'):raise ValueError('T1 private/public receipt differs')
    json.dump(value, sys.stdout, indent=2, sort_keys=True);sys.stdout.write('\n')
