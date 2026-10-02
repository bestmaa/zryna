"""Fixed compiler process-tree confinement below an independently authenticated toolkit."""

import ctypes
import os
import pathlib
import platform
import selectors
import signal
import subprocess
import sys
import time
from cgroup import JobGroup, PolicyError
from errors import cleanup_failure
from request import validate_source_request
from seccomp import compiler_filter

MAX_FRAME = 1_277_956
MAX_STDERR = 4096


def compile_source(request, closure, delegated, host):
    started = time.monotonic()
    execution_deadline = started + 10
    overall_deadline = started + 12
    owned_fds = []
    handlers = {}
    group = process = None
    primary = cleanup = result = None
    aborted = False

    def cancel(_number, _frame):
        nonlocal aborted
        aborted = True

    def reap():
        if process:
            process.poll()
        while True:
            try:
                pid, status = os.waitpid(-1, os.WNOHANG)
            except ChildProcessError:
                return False
            if pid == 0:
                return True
            if process and pid == process.pid:
                process.returncode = os.waitstatus_to_exitcode(status)

    def acquire_fd(fd):
        owned_fds.append(fd)
        return fd

    try:
        validate_source_request(request)
        if platform.system() != "Linux" or platform.machine() != "x86_64":
            raise PolicyError("PLAYGROUND-UNSUPPORTED-HOST")
        host.revalidate()
        # Adopt and reap descendants after a reaper dies; populated=0 alone is insufficient.
        if ctypes.CDLL(None, use_errno=True).prctl(36, 1, 0, 0, 0) != 0:
            raise PolicyError("PLAYGROUND-SUBREAPER")
        group = JobGroup(delegated, 536_870_912, 32)
        ready_read, ready_write = os.pipe2(os.O_CLOEXEC)
        acquire_fd(ready_read)
        acquire_fd(ready_write)
        filter_fd = acquire_fd(os.memfd_create("playground-seccomp", os.MFD_CLOEXEC))
        policy = compiler_filter()
        if os.write(filter_fd, policy) != len(policy):
            raise PolicyError("PLAYGROUND-SECCOMP-WRITE")
        os.lseek(filter_fd, 0, os.SEEK_SET)
        for number in [signal.SIGINT, signal.SIGTERM]:
            previous = signal.getsignal(number)
            signal.signal(number, cancel)
            handlers[number] = previous
        if time.monotonic() >= execution_deadline:
            raise PolicyError("PLAYGROUND-COMPILER-DEADLINE")
        command = [host.bubblewrap, "--unshare-all", "--unshare-user", "--disable-userns",
                   "--assert-userns-disabled", "--new-session", "--die-with-parent",
                   "--cap-drop", "ALL", "--clearenv", "--setenv", "LANG", "C.UTF-8",
                   "--proc", "/proc", "--dev", "/dev", "--size", "67108864", "--tmpfs", "/tmp",
                   *closure.arguments(), "--remount-ro", "/", "--chdir", "/materials",
                   "--seccomp", str(filter_fd), "--", "/app/compiler", "--materials-root", "/materials"]
        join = str(pathlib.Path(__file__).with_name("join.py"))
        process = subprocess.Popen([host.python, "-I", "-S", "-B", join, str(group.fd), str(ready_write),
                                    *command], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                   stderr=subprocess.PIPE, env={}, start_new_session=True,
                                   close_fds=True, pass_fds=(group.fd, ready_write, filter_fd, *closure.fds))
        os.close(ready_write)
        owned_fds.remove(ready_write)
        streams = {ready_read: "ready", process.stdout.fileno(): "stdout",
                   process.stderr.fileno(): "stderr"}
        output = {"stdout": bytearray(), "stderr": bytearray()}
        offset = 0
        with selectors.DefaultSelector() as selector:
            for fd, role in streams.items():
                os.set_blocking(fd, False)
                selector.register(fd, selectors.EVENT_READ, role)
            while selector.get_map():
                if aborted:
                    raise PolicyError("PLAYGROUND-CANCELLED")
                if time.monotonic() >= execution_deadline:
                    raise PolicyError("PLAYGROUND-COMPILER-DEADLINE")
                if group.exceeded():
                    raise PolicyError("PLAYGROUND-COMPILER-RESOURCE")
                for key, _mask in selector.select(min(0.05, max(0, execution_deadline - time.monotonic()))):
                    fd, role = key.fd, key.data
                    if role == "stdin":
                        offset += os.write(fd, request[offset:offset + 4096])
                        if offset == len(request):
                            selector.unregister(fd)
                            process.stdin.close()
                        continue
                    data = os.read(fd, 65_536)
                    if not data:
                        selector.unregister(fd)
                        if role == "ready":
                            raise PolicyError("PLAYGROUND-CGROUP-MEMBERSHIP")
                        continue
                    if role == "ready":
                        if data != b"1":
                            raise PolicyError("PLAYGROUND-CGROUP-MEMBERSHIP")
                        selector.unregister(fd)
                        os.set_blocking(process.stdin.fileno(), False)
                        selector.register(process.stdin, selectors.EVENT_WRITE, "stdin")
                    else:
                        output[role].extend(data)
                        if len(output[role]) > (MAX_FRAME if role == "stdout" else MAX_STDERR):
                            raise PolicyError("PLAYGROUND-COMPILER-OUTPUT-LIMIT")
        if process.poll() is None:
            process.wait(timeout=max(0.001, execution_deadline - time.monotonic()))
        if process.returncode != 0 or output["stderr"] or group.exceeded():
            raise PolicyError("PLAYGROUND-COMPILER-FAILED")
        result = bytes(output["stdout"])
        host.revalidate()
    except BaseException as error:
        primary = error if isinstance(error, PolicyError) else PolicyError("PLAYGROUND-HOST-FAILURE")
        if primary is not error and getattr(error, "cleanup", None) is not None:
            primary.cleanup = error.cleanup
    finally:
        try:
            # Kill startup failures that have not reached the membership handshake as well.
            if process and process.poll() is None:
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
            if group:
                group.close(reap, min(overall_deadline, time.monotonic() + 2))
        except BaseException as error:
            cleanup = error
        for number, handler in handlers.items():
            try:
                signal.signal(number, handler)
            except BaseException as error:
                cleanup = cleanup or error
        for fd in reversed(owned_fds):
            try:
                os.close(fd)
            except BaseException as error:
                cleanup = cleanup or error
        if process:
            for stream in [process.stdin, process.stdout, process.stderr]:
                try:
                    if not stream.closed:
                        stream.close()
                except BaseException as error:
                    cleanup = cleanup or error
    if cleanup:
        raise cleanup_failure(primary, cleanup)
    if primary:
        raise primary
    return result
