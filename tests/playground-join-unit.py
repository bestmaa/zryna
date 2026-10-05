"""Mocked bootstrap contract checks; no cgroup write, privilege change or executable run."""

from contextlib import ExitStack
import os
import pathlib
import resource
import sys
import unittest
from unittest.mock import patch

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1] /
                      "examples/playground/restricted"))
import join


GROUP, READY, WRITER, READER, PID = 80, 81, 82, 83, 7342
COMMAND = ["/trusted/bubblewrap", "--unshare-all", "--", "/app/compiler"]


class ExecutionRequested(BaseException):
    pass


class Bootstrap:
    """Record syscall order and transient descriptor ownership without OS operations."""
    def __init__(self):
        self.events = []
        self.live = set()
        self.membership = f"{PID}\n"
        self.fail = None
        self.short = None
        self.privileges = 0
        self.inherited_limits = {resource.RLIMIT_CORE: (0, 0),
                                 resource.RLIMIT_FSIZE: (268_435_456, 268_435_456),
                                 resource.RLIMIT_NOFILE: (4096, 4096)}
        self.applied_limits = {}
        self.readback_values = {}
        self.stack = ExitStack()

    def __enter__(self):
        self.stack.enter_context(patch("join.sys.argv", ["join.py", str(GROUP), str(READY), *COMMAND]))
        self.stack.enter_context(patch("join.os.getpid", return_value=PID))
        self.stack.enter_context(patch("join.os.open", side_effect=self.open))
        self.stack.enter_context(patch("join.os.close", side_effect=self.close))
        self.stack.enter_context(patch("join.os.fdopen", side_effect=self.fdopen))
        self.stack.enter_context(patch("join.os.write", side_effect=self.write))
        self.stack.enter_context(patch("join.os.execve", side_effect=self.execute))
        self.stack.enter_context(patch("join.resource.setrlimit", side_effect=self.limit))
        self.stack.enter_context(patch("join.resource.getrlimit", side_effect=self.get_limit))
        self.stack.enter_context(patch("join.os.environ", {"LD_PRELOAD": "/ignored", "UNTRUSTED": "test-only"}))
        library = self.stack.enter_context(patch("join.ctypes.CDLL"))
        library.return_value.prctl.side_effect = self.prctl
        self.library = library
        return self

    def __exit__(self, *_args):
        self.stack.close()

    def open(self, name, flags, *, dir_fd):
        assert name == "cgroup.procs"
        assert dir_fd == GROUP
        assert flags in [os.O_WRONLY | os.O_NOFOLLOW, os.O_RDONLY | os.O_NOFOLLOW]
        role = "membership-write-open" if flags & os.O_WRONLY else "membership-read-open"
        self.events.append((role,))
        if self.fail == role:
            raise OSError(role)
        fd = WRITER if flags & os.O_WRONLY else READER
        self.live.add(fd)
        return fd

    def close(self, fd):
        self.events.append(("close", fd))
        self.live.discard(fd)

    def fdopen(self, fd):
        assert fd == READER
        self.events.append(("fdopen", fd))
        if self.fail == "fdopen":
            raise OSError("fdopen")
        bootstrap = self
        class Members:
            def __enter__(self):
                return self
            def read(self):
                bootstrap.events.append(("membership-read",))
                if bootstrap.fail == "membership-read":
                    raise OSError("membership-read")
                return bootstrap.membership
            def __exit__(self, *_args):
                bootstrap.close(fd)
        return Members()

    def write(self, fd, value):
        role = "membership-write" if fd == WRITER else "ready"
        assert fd in [WRITER, READY]
        assert value == (str(PID).encode("ascii") if fd == WRITER else b"1")
        self.events.append((role, fd, value))
        if self.fail == role:
            raise OSError(role)
        return self.short if self.short is not None and self.fail == role + "-short" else len(value)

    def limit(self, kind, value):
        self.events.append(("limit", kind, value))
        if self.fail == kind:
            raise OSError("limit")
        self.applied_limits[kind] = value

    def get_limit(self, kind):
        role = "limit-readback" if kind in self.applied_limits else "limit-inherited"
        self.events.append((role, kind))
        if self.fail == (role, kind):
            raise OSError(role)
        if role == "limit-readback":
            return self.readback_values.get(kind, self.applied_limits[kind])
        return self.inherited_limits[kind]

    def prctl(self, *arguments):
        assert arguments == (38, 1, 0, 0, 0)
        self.events.append(("no-new-privileges",))
        return self.privileges

    def execute(self, path, argv, environment):
        self.events.append(("exec", path, argv, environment))
        raise ExecutionRequested()


@unittest.skipUnless(sys.platform == "linux", "Linux bootstrap resource constants")
class JoinContract(unittest.TestCase):
    def assert_not_ready_or_executed(self, bootstrap):
        self.assertFalse(any(event[0] in ["ready", "exec"] for event in bootstrap.events))

    def test_membership_limits_privileges_and_descriptor_closure_precede_exec(self):
        with Bootstrap() as bootstrap:
            bootstrap.membership = f"123\n{PID}\n999\n"
            with self.assertRaises(ExecutionRequested):
                join.main()
            self.assertEqual(bootstrap.events, [
                ("membership-write-open",), ("membership-write", WRITER, b"7342"), ("close", WRITER),
                ("membership-read-open",), ("fdopen", READER), ("membership-read",), ("close", READER),
                ("limit-inherited", resource.RLIMIT_CORE),
                ("limit", resource.RLIMIT_CORE, (0, 0)), ("limit-readback", resource.RLIMIT_CORE),
                ("limit-inherited", resource.RLIMIT_FSIZE),
                ("limit", resource.RLIMIT_FSIZE, (268_435_456, 268_435_456)),
                ("limit-readback", resource.RLIMIT_FSIZE),
                ("limit-inherited", resource.RLIMIT_NOFILE),
                ("limit", resource.RLIMIT_NOFILE, (512, 512)),
                ("limit-readback", resource.RLIMIT_NOFILE), ("no-new-privileges",),
                ("ready", READY, b"1"), ("close", READY), ("close", GROUP),
                ("exec", COMMAND[0], COMMAND, {"LANG": "C.UTF-8", "LC_ALL": "C.UTF-8"})])
            bootstrap.library.assert_called_once_with(None, use_errno=True)
            self.assertEqual(bootstrap.live, set())

    def test_membership_write_and_read_failures_release_transient_descriptors(self):
        for site in ["membership-write-open", "membership-write", "membership-read-open", "membership-read"]:
            with self.subTest(site=site), Bootstrap() as bootstrap:
                bootstrap.fail = site
                with self.assertRaisesRegex(OSError, site):
                    join.main()
                self.assertEqual(bootstrap.live, set())
                self.assert_not_ready_or_executed(bootstrap)
                self.assertFalse(any(event[0] == "limit" for event in bootstrap.events))

    def test_readback_requires_exact_pid_line_before_limits_and_readiness(self):
        for members in ["", "73420\n", "17342\n", " 7342\n", "7342 \n"]:
            with self.subTest(members=members), Bootstrap() as bootstrap:
                bootstrap.membership = members
                with self.assertRaisesRegex(RuntimeError, "PLAYGROUND-CGROUP-MEMBERSHIP"):
                    join.main()
                self.assertEqual(bootstrap.live, set())
                self.assert_not_ready_or_executed(bootstrap)
                self.assertFalse(any(event[0] == "limit" for event in bootstrap.events))

    def test_every_limit_failure_stops_before_readiness_and_execution(self):
        for kind in [resource.RLIMIT_CORE, resource.RLIMIT_FSIZE, resource.RLIMIT_NOFILE]:
            with self.subTest(kind=kind), Bootstrap() as bootstrap:
                bootstrap.fail = kind
                with self.assertRaisesRegex(OSError, "limit"):
                    join.main()
                self.assertEqual(bootstrap.live, set())
                self.assert_not_ready_or_executed(bootstrap)
                self.assertFalse(any(event[0] == "no-new-privileges" for event in bootstrap.events))

    def test_no_new_privileges_failure_stops_before_readiness_and_execution(self):
        with Bootstrap() as bootstrap:
            bootstrap.privileges = -1
            with self.assertRaisesRegex(RuntimeError, "PLAYGROUND-NO-NEW-PRIVILEGES"):
                join.main()
            self.assertEqual(bootstrap.live, set())
            self.assert_not_ready_or_executed(bootstrap)
            self.assertEqual(sum(event[0] == "limit" for event in bootstrap.events), 3)

    def test_infinite_or_insufficient_hard_limits_never_attempt_an_increase(self):
        for kind, value in [(resource.RLIMIT_CORE, (0, resource.RLIM_INFINITY)),
                            (resource.RLIMIT_FSIZE, (1_048_576, 1_048_576)),
                            (resource.RLIMIT_NOFILE, (128, 128))]:
            with self.subTest(kind=kind), Bootstrap() as bootstrap:
                bootstrap.inherited_limits[kind] = value
                with self.assertRaisesRegex(RuntimeError, "PLAYGROUND-RUNTIME-LIMIT"):
                    join.main()
                self.assertFalse(any(event[0] == "limit" and event[1] == kind
                                     for event in bootstrap.events))
                self.assert_not_ready_or_executed(bootstrap)

    def test_each_inherited_read_or_readback_error_prevents_readiness(self):
        for role in ["limit-inherited", "limit-readback"]:
            for kind in [resource.RLIMIT_CORE, resource.RLIMIT_FSIZE, resource.RLIMIT_NOFILE]:
                with self.subTest(role=role, kind=kind), Bootstrap() as bootstrap:
                    bootstrap.fail = (role, kind)
                    with self.assertRaisesRegex(OSError, role):
                        join.main()
                    self.assert_not_ready_or_executed(bootstrap)

    def test_changed_soft_or_hard_readback_prevents_readiness(self):
        for kind in [resource.RLIMIT_CORE, resource.RLIMIT_FSIZE, resource.RLIMIT_NOFILE]:
            for value in [(1, 0), (0, 1), (resource.RLIM_INFINITY, resource.RLIM_INFINITY)]:
                with self.subTest(kind=kind, value=value), Bootstrap() as bootstrap:
                    bootstrap.readback_values[kind] = value
                    with self.assertRaisesRegex(RuntimeError, "PLAYGROUND-RUNTIME-LIMIT"):
                        join.main()
                    self.assert_not_ready_or_executed(bootstrap)

    def test_ready_write_failure_cannot_execute_and_membership_descriptors_are_closed(self):
        with Bootstrap() as bootstrap:
            bootstrap.fail = "ready"
            with self.assertRaisesRegex(OSError, "ready"):
                join.main()
            self.assertEqual(bootstrap.live, set())
            self.assertFalse(any(event[0] == "exec" for event in bootstrap.events))

    def test_fdopen_failure_disposes_the_already_opened_read_descriptor(self):
        with Bootstrap() as bootstrap:
            bootstrap.fail = "fdopen"
            with self.assertRaisesRegex(OSError, "fdopen"):
                join.main()
            self.assertEqual(bootstrap.live, set())
            self.assert_not_ready_or_executed(bootstrap)

    def test_short_membership_write_rejects_even_if_readback_already_contains_pid(self):
        with Bootstrap() as bootstrap:
            bootstrap.fail = "membership-write-short"
            bootstrap.short = 1
            with self.assertRaisesRegex(RuntimeError, "PLAYGROUND-CGROUP-MEMBERSHIP"):
                join.main()
            self.assertEqual(bootstrap.live, set())
            self.assert_not_ready_or_executed(bootstrap)

    def test_zero_byte_ready_write_cannot_execute(self):
        with Bootstrap() as bootstrap:
            bootstrap.fail = "ready-short"
            bootstrap.short = 0
            with self.assertRaisesRegex(RuntimeError, "PLAYGROUND-CGROUP-MEMBERSHIP"):
                join.main()
            self.assertEqual(bootstrap.live, set())
            self.assertFalse(any(event[0] == "exec" for event in bootstrap.events))


if __name__ == "__main__":
    unittest.main()
