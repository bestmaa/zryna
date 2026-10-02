"""Trusted bootstrap: join the owned group before exposing the child to input."""

import ctypes
import os
import resource
import sys


def main():
    # These inherited descriptors and argv are selected only by the fixed host supervisor.
    group_fd, ready_fd, *command = sys.argv[1:]
    group_fd, ready_fd = int(group_fd), int(ready_fd)
    fd = os.open("cgroup.procs", os.O_WRONLY | os.O_NOFOLLOW, dir_fd=group_fd)
    try:
        membership = str(os.getpid()).encode("ascii")
        if os.write(fd, membership) != len(membership):
            raise RuntimeError("PLAYGROUND-CGROUP-MEMBERSHIP")
    finally:
        os.close(fd)
    member_fd = os.open("cgroup.procs", os.O_RDONLY | os.O_NOFOLLOW, dir_fd=group_fd)
    try:
        members = os.fdopen(member_fd)
    except BaseException:
        os.close(member_fd)
        raise
    with members:
        if str(os.getpid()) not in members.read().splitlines():
            raise RuntimeError("PLAYGROUND-CGROUP-MEMBERSHIP")
    # Authenticated FD-copy setup may stage a 256 MiB entry and 128 material descriptors.
    # After mount setup, the compiler stages its provider under 12 MiB FSIZE, then
    # tightens to runtime limits before any Node probe or source compilation.
    for kind, limit in [(resource.RLIMIT_CORE, 0),
                        (resource.RLIMIT_FSIZE, 268_435_456),
                        (resource.RLIMIT_NOFILE, 512)]:
        inherited = resource.getrlimit(kind)
        if inherited[1] == resource.RLIM_INFINITY or inherited[1] < limit:
            raise RuntimeError("PLAYGROUND-RUNTIME-LIMIT")
        resource.setrlimit(kind, (limit, limit))
        if resource.getrlimit(kind) != (limit, limit):
            raise RuntimeError("PLAYGROUND-RUNTIME-LIMIT")
    # libseccomp/BPF is applied by bubblewrap to the isolated executable after setup.
    if ctypes.CDLL(None, use_errno=True).prctl(38, 1, 0, 0, 0) != 0:
        raise RuntimeError("PLAYGROUND-NO-NEW-PRIVILEGES")
    if os.write(ready_fd, b"1") != 1:
        raise RuntimeError("PLAYGROUND-CGROUP-MEMBERSHIP")
    os.close(ready_fd)
    os.close(group_fd)
    os.execve(command[0], command, {"LANG": "C.UTF-8", "LC_ALL": "C.UTF-8"})


if __name__ == "__main__":
    main()
