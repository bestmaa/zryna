"""Light admission/sealed-capture checks; no compiler/browser/isolation acceptance claims."""

import hashlib
import json
import os
import pathlib
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1] /
                      "examples/playground/restricted"))
from cgroup import PolicyError
from closure import CapturedClosure
from request import validate_source_request
from request import structural_budget
from seccomp import compiler_filter
from supervisor import compile_source


class CapturedHostFixture:
    python = "/usr/bin/python3"
    bubblewrap = "/usr/bin/bwrap"

    def revalidate(self):
        pass


class HostAdmission(unittest.TestCase):
    def test_closed_request_and_exact_escaped_source(self):
        escaped = (b'{"version":1,"revision":9007199254740991,"source":"' +
                   b"\\u0000" * 4096 + b'"}')
        self.assertLess(len(escaped), 32_768)
        self.assertEqual(validate_source_request(escaped)["source"], "\0" * 4096)
        exact = escaped + b" " * (32_768 - len(escaped))
        self.assertEqual(validate_source_request(exact)["revision"], 9_007_199_254_740_991)
        for data in [exact + b" ", b"\xff", b"[]", b"{}", b"null",
                     b'{"version":1,"revision":1,"source":"","sou\\u0072ce":"other"}',
                     b'{"version":1,"revision":true,"source":""}',
                     b'{"version":1,"revision":1e0,"source":""}',
                     b'{"version":1,"revision":NaN,"source":""}',
                     b'\xef\xbb\xbf{"version":1,"revision":1,"source":""}',
                     b'{"version":1,"revision":1,"source":"\\ud800"}',
                     b'{"version":1,"revision":1,"source":""}{}',
                     json.dumps({"version": 1, "revision": 1, "source": "x" * 4097}).encode(),
                     json.dumps({"version": 1, "revision": 1, "source": "", "target": "native"}).encode()]:
            with self.assertRaises(PolicyError):
                validate_source_request(data)

    def test_malformed_request_cannot_allocate_group_or_launch_process(self):
        with patch("supervisor.JobGroup") as group, patch("supervisor.subprocess.Popen") as launch:
            with self.assertRaises(PolicyError):
                compile_source(b"{}", None, "/not-used", None)
            group.assert_not_called()
            launch.assert_not_called()
        self.assertEqual(validate_source_request(
            br'{"version":1,"revision":2,"source":"\u00e9\r\n"}')["source"], "é\r\n")

    def test_structure_boundaries_reject_before_json_object_construction(self):
        structural_budget(b"[" * 8 + b"0" + b"]" * 8)
        structural_budget(b"[" + b"0," * 511 + b"0]")
        for data in [b"[" * 9 + b"0" + b"]" * 9,
                     b"[" + b"0," * 512 + b"0]", b"0" * 16_385]:
            with patch("request.json.loads") as decoder:
                with self.assertRaises(PolicyError):
                    validate_source_request(data)
                decoder.assert_not_called()

    @unittest.skipUnless(sys.platform == "linux", "sealed Linux memfd capture only")
    def test_captured_bytes_are_sealed_and_subsequent_path_replacement_cannot_substitute(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            content = b"transport-only fixture; never executed"
            (root / "compiler").write_bytes(content)
            (root / "node").write_bytes(content)
            entries = [{"path": name, "mount": mount, "bytes": len(content),
                        "sha256": hashlib.sha256(content).hexdigest(), "executable": True}
                       for name, mount in [("compiler", "/app/compiler"),
                                           ("node", "/materials/runtime/node/bin/node")]]
            captured = CapturedClosure(root, entries)
            try:
                (root / "compiler").write_bytes(b"substituted")
                self.assertEqual(os.read(captured.fds[0], 4096), content)
                with self.assertRaises(OSError):
                    os.write(captured.fds[0], b"evil")
                with self.assertRaises(PolicyError):
                    CapturedClosure(root, entries)
                (root / "compiler").unlink()
                (root / "compiler").symlink_to(root / "node")
                with self.assertRaises((PolicyError, OSError)):
                    CapturedClosure(root, entries)
            finally:
                captured.close()

    @unittest.skipUnless(sys.platform == "linux", "Linux x86-64 seccomp policy only")
    def test_syscall_filter_checks_architecture_and_has_default_kill(self):
        import struct
        instructions = [struct.unpack("HBBI", compiler_filter()[offset:offset + 8])
                        for offset in range(0, len(compiler_filter()), 8)]
        self.assertEqual(instructions[0], (0x20, 0, 0, 4))
        self.assertEqual(instructions[1][3], 0xC000003E)
        self.assertEqual(instructions[-1], (0x06, 0, 0, 0x80000000))
        allowed = {instructions[index][3] for index in range(4, len(instructions) - 3, 2)}
        for prohibited in [41, 42, 43, 101, 165, 166, 248, 249, 250, 272, 288, 298, 308, 310, 311, 321]:
            self.assertNotIn(prohibited, allowed)

    @unittest.skipUnless(sys.platform == "linux", "Linux descriptor lifecycle")
    def test_injected_acquisition_failures_close_every_owned_descriptor_and_recover(self):
        from contextlib import ExitStack
        valid = b'{"version":1,"revision":1,"source":""}'

        class FakeGroup:
            def __init__(self, *_args):
                self.fd = os.open("/dev/null", os.O_RDONLY)
            def close(self, reap, deadline):
                self.assertions = (reap(), deadline)
                os.close(self.fd)

        class Closure:
            fds = []
            def arguments(self):
                return []

        for site in ["supervisor.os.pipe2", "supervisor.os.memfd_create", "supervisor.os.write",
                     "supervisor.compiler_filter", "supervisor.signal.signal", "supervisor.subprocess.Popen"]:
            before = len(os.listdir("/proc/self/fd"))
            with ExitStack() as stack:
                stack.enter_context(patch("supervisor.JobGroup", FakeGroup))
                library = stack.enter_context(patch("supervisor.ctypes.CDLL"))
                library.return_value.prctl.return_value = 0
                stack.enter_context(patch("supervisor.subprocess.Popen", side_effect=OSError("injected spawn")))
                stack.enter_context(patch(site, side_effect=OSError("injected setup")))
                with self.assertRaises(PolicyError) as failure:
                    compile_source(valid, Closure(), "/unused", CapturedHostFixture())
                self.assertEqual(str(failure.exception), "PLAYGROUND-HOST-FAILURE")
            self.assertEqual(len(os.listdir("/proc/self/fd")), before, site)
            self.assertEqual(validate_source_request(valid)["revision"], 1)

    @unittest.skipUnless(sys.platform == "linux", "Linux cleanup failure evidence")
    def test_primary_failure_keeps_separate_cleanup_failure_without_returning_frame(self):
        class FailedGroup:
            fd = -1
            def close(self, *_args):
                raise PolicyError("PLAYGROUND-HOST-TEARDOWN")
        with patch("supervisor.JobGroup", return_value=FailedGroup()), \
             patch("supervisor.ctypes.CDLL") as library, \
             patch("supervisor.os.pipe2", side_effect=OSError("injected setup")):
            library.return_value.prctl.return_value = 0
            with self.assertRaises(PolicyError) as failure:
                compile_source(b'{"version":1,"revision":1,"source":""}', None, "/unused", CapturedHostFixture())
            self.assertEqual(str(failure.exception), "PLAYGROUND-HOST-FAILURE")
            self.assertEqual(str(failure.exception.cleanup), "PLAYGROUND-HOST-TEARDOWN")

    @unittest.skipUnless(sys.platform == "linux", "Linux handshake and descriptor lifecycle")
    def test_injected_handshake_selector_and_child_failures_reap_and_close_before_fresh_admission(self):
        from contextlib import ExitStack
        import io
        valid = b'{"version":1,"revision":1,"source":""}'

        class FakeGroup:
            def __init__(self, *_args):
                self.fd = os.open("/dev/null", os.O_RDONLY)
            def exceeded(self):
                return False
            def close(self, reap, deadline):
                self.reaped = not reap()
                os.close(self.fd)

        class Closure:
            fds = []
            def arguments(self):
                return []

        class Process:
            pid = -1
            returncode = 1
            def __init__(self, command, ready):
                if ready is not None:
                    os.write(int(command[6]), ready)
                input_read, input_write = os.pipe()
                os.close(input_read)
                self.stdin = io.open(input_write, "wb", buffering=0)
                self.stdout = self.stream()
                self.stderr = self.stream()
            @staticmethod
            def stream():
                read, write = os.pipe()
                os.close(write)
                return io.open(read, "rb", buffering=0)
            def poll(self):
                return self.returncode

        for ready, selector_failure, expected in [(None, False, "PLAYGROUND-CGROUP-MEMBERSHIP"),
                                                   (b"X", False, "PLAYGROUND-CGROUP-MEMBERSHIP"),
                                                   (b"1", True, "PLAYGROUND-HOST-FAILURE")]:
            before = len(os.listdir("/proc/self/fd"))
            with ExitStack() as stack:
                stack.enter_context(patch("supervisor.JobGroup", FakeGroup))
                library = stack.enter_context(patch("supervisor.ctypes.CDLL"))
                library.return_value.prctl.return_value = 0
                stack.enter_context(patch("supervisor.subprocess.Popen",
                                          side_effect=lambda command, **_kwargs: Process(command, ready)))
                if selector_failure:
                    stack.enter_context(patch("supervisor.selectors.DefaultSelector", side_effect=OSError("selector")))
                with self.assertRaises(PolicyError) as failure:
                    compile_source(valid, Closure(), "/unused", CapturedHostFixture())
                self.assertEqual(str(failure.exception), expected)
            self.assertEqual(len(os.listdir("/proc/self/fd")), before)
            self.assertEqual(validate_source_request(valid)["revision"], 1)

    @unittest.skipUnless(sys.platform == "linux", "Linux constructor failure evidence")
    def test_wrapped_constructor_failure_retains_prior_cleanup_evidence(self):
        original = OSError("constructor acquisition")
        original.cleanup = PolicyError("PLAYGROUND-CGROUP-REPLACED")
        with patch("supervisor.JobGroup", side_effect=original), \
             patch("supervisor.ctypes.CDLL") as library:
            library.return_value.prctl.return_value = 0
            with self.assertRaises(PolicyError) as failure:
                compile_source(b'{"version":1,"revision":1,"source":""}', None,
                               "/unused", CapturedHostFixture())
        self.assertEqual(str(failure.exception), "PLAYGROUND-HOST-FAILURE")
        self.assertIs(failure.exception.cleanup, original.cleanup)

    def test_multiple_cleanup_failures_preserve_each_independent_cause(self):
        from errors import cleanup_failure
        original = PolicyError("PLAYGROUND-COMPILER-DEADLINE")
        first, second = OSError("kill failure"), OSError("descriptor failure")
        cleanup_failure(original, first)
        self.assertIs(cleanup_failure(original, second), original)
        self.assertEqual(original.cleanup.exceptions, (first, second))

    @unittest.skipUnless(sys.platform == "linux", "Linux descriptor disposal")
    def test_material_and_helper_close_attempt_every_owned_descriptor(self):
        from host import HostCapability
        native_close = os.close
        for capture in [CapturedClosure.__new__(CapturedClosure), HostCapability.__new__(HostCapability)]:
            before = len(os.listdir("/proc/self/fd"))
            descriptors = [os.open("/dev/null", os.O_RDONLY) for _ in range(3)]
            capture.root_fd = descriptors[-1]
            if isinstance(capture, CapturedClosure):
                capture.fds = descriptors[:-1]
            else:
                capture.files = [(fd, {}) for fd in descriptors[:-1]]
            attempted = []
            def fail_after_close(fd):
                attempted.append(fd)
                native_close(fd)
                raise OSError("reported close error")
            with patch("errors.os.close", side_effect=fail_after_close):
                with self.assertRaises(PolicyError) as failure:
                    capture.close()
            self.assertEqual(attempted, descriptors)
            self.assertIsNotNone(failure.exception.cleanup)
            self.assertEqual(len(os.listdir("/proc/self/fd")), before)
            self.assertIsNone(capture.root_fd)
            capture.close()

    @unittest.skipUnless(sys.platform == "linux", "Linux sealed material acquisition")
    def test_source_close_failure_preserves_material_rejection_and_releases_retained_copy(self):
        native_close = os.close
        with tempfile.TemporaryDirectory() as temporary:
            path = pathlib.Path(temporary) / "material"
            content = b"ordinary unexecuted material"
            path.write_bytes(content)
            source = os.open(path, os.O_RDONLY)
            captured = CapturedClosure.__new__(CapturedClosure)
            captured.fds, captured.mounts, captured.root_fd = [], [], None
            cleanup = OSError("reported source close error")
            attempted = []

            def fail_after_close(fd):
                attempted.append(fd)
                native_close(fd)
                raise cleanup

            entry = {"path": "material", "mount": "/app/compiler", "bytes": len(content),
                     "sha256": "0" * 64, "executable": True}
            try:
                with patch("closure.regular", return_value=source), \
                        patch("closure.os.close", side_effect=fail_after_close):
                    with self.assertRaises(PolicyError) as failure:
                        captured.capture(entry)
                self.assertEqual(str(failure.exception), "PLAYGROUND-MATERIAL-HASH")
                self.assertIs(failure.exception.cleanup, cleanup)
                self.assertEqual(attempted, [source])
                retained = list(captured.fds)
            finally:
                captured.close()
            for descriptor in retained:
                with self.assertRaises(OSError):
                    os.fstat(descriptor)


if __name__ == "__main__":
    unittest.main()
