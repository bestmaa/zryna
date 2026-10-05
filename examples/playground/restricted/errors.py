"""Separate policy failure and cleanup evidence without fabricated source diagnostics."""

import os

class PolicyError(RuntimeError):
    def __init__(self, reason, cleanup=None):
        super().__init__(reason)
        self.cleanup = cleanup


def cleanup_failure(primary, cleanup):
    if primary is None:
        return PolicyError("PLAYGROUND-HOST-TEARDOWN", cleanup)
    existing = getattr(primary, "cleanup", None)
    primary.cleanup = (BaseExceptionGroup("PLAYGROUND-CLEANUP-FAILURES", [existing, cleanup])
                       if existing is not None else cleanup)
    return primary


def close_descriptors(descriptors):
    """Attempt each independently owned close exactly once, preserving every failure."""
    failure = None
    for descriptor in descriptors:
        try:
            os.close(descriptor)
        except BaseException as error:
            failure = cleanup_failure(failure, error)
    if failure is not None:
        raise failure
