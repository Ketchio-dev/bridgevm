"""Auxiliary cancellation must escape subprocess wait helpers' OSError fallbacks."""


class AuxiliaryInterrupted(RuntimeError):
    """Unwind an auxiliary command through its owned-process cleanup."""


def interrupted(_signal, _frame):
    raise AuxiliaryInterrupted("auxiliary orchestration interrupted")
