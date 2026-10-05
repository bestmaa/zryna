"""An owned domain below an existing operator-delegated cgroup v2 subtree."""

import os
import pathlib
import time
import uuid
from errors import PolicyError, cleanup_failure
from handles import control, directory


def cpu_set(value):
    result = set()
    for part in value.split(","):
        if not part:
            continue
        ends = part.split("-")
        if len(ends) > 2 or not all(end.isdigit() for end in ends):
            raise PolicyError("PLAYGROUND-CPU-SET")
        first, last = int(ends[0]), int(ends[-1])
        if first > last or last > 65_535:
            raise PolicyError("PLAYGROUND-CPU-SET")
        result.update(range(first, last + 1))
    return result


class JobGroup:
    def __init__(self, delegated, memory_bytes, tasks, clock=time.monotonic):
        self.clock = clock
        self.closed = False
        self.fd = self.root_fd = self.name = self.identity = None
        self.root = pathlib.Path(delegated)
        if not self.root.is_absolute() or str(self.root) != str(pathlib.PurePosixPath(delegated)):
            raise PolicyError("PLAYGROUND-CGROUP-ROOT")
        try:
            self.root_fd = directory(str(self.root))
            observed = os.fstat(self.root_fd)
            if observed.st_uid != os.getuid():
                raise PolicyError("PLAYGROUND-CGROUP-OWNER")
            matched = False
            for line in pathlib.Path("/proc/self/mountinfo").read_text().splitlines():
                if " - cgroup2 " not in line:
                    continue
                fields = line.split()
                mount = pathlib.Path(fields[4])
                if not self.root.is_relative_to(mount):
                    continue
                mount_fd = directory(str(mount))
                try:
                    mounted = os.fstat(mount_fd)
                    if observed.st_dev != mounted.st_dev:
                        continue
                    if fields[3] == "/" and observed.st_ino == mounted.st_ino:
                        raise PolicyError("PLAYGROUND-CGROUP-DELEGATION")
                    matched = True
                finally:
                    os.close(mount_fd)
            if not matched or control(self.root_fd, "cgroup.type") != "domain":
                raise PolicyError("PLAYGROUND-CGROUP-FILESYSTEM")
            if not {"cpu", "cpuset", "memory", "pids"} <= set(
                    control(self.root_fd, "cgroup.subtree_control").split()):
                raise PolicyError("PLAYGROUND-CGROUP-CONTROLLERS")
            if control(self.root_fd, "cgroup.procs"):
                raise PolicyError("PLAYGROUND-CGROUP-DELEGATION")
            cpus = sorted(cpu_set(control(self.root_fd, "cpuset.cpus.effective")))[:2]
            mems = control(self.root_fd, "cpuset.mems.effective")
            if len(cpus) != 2 or not cpu_set(mems):
                raise PolicyError("PLAYGROUND-CPU-CONTROLS")
            candidate = "playground-" + uuid.uuid4().hex
            os.mkdir(candidate, mode=0o700, dir_fd=self.root_fd)
            self.name = candidate
            self.identity = os.stat(self.name, dir_fd=self.root_fd, follow_symlinks=False)
            self.fd = os.open(self.name, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW,
                              dir_fd=self.root_fd)
            opened = os.fstat(self.fd)
            if (opened.st_dev, opened.st_ino) != (self.identity.st_dev, self.identity.st_ino):
                raise PolicyError("PLAYGROUND-CGROUP-REPLACED")
            self.path = self.root / self.name
            for name, value in [("memory.max", memory_bytes), ("memory.swap.max", 0),
                                ("pids.max", tasks), ("cpu.max", "200000 100000"),
                                ("cpuset.mems", mems), ("cpuset.cpus", ",".join(map(str, cpus)))]:
                control(self.fd, name, value)
                valid = (cpu_set(control(self.fd, name)) == cpu_set(str(value)) if
                         name.startswith("cpuset.") else control(self.fd, name) == str(value))
                if not valid:
                    raise PolicyError("PLAYGROUND-CGROUP-LIMIT")
            if cpu_set(control(self.fd, "cpuset.cpus.effective")) != set(cpus):
                raise PolicyError("PLAYGROUND-CPU-CONTROLS")
            kill_fd = os.open("cgroup.kill", os.O_WRONLY | os.O_NOFOLLOW, dir_fd=self.fd)
            os.close(kill_fd)
            self.baseline = self.events()
        except BaseException as primary:
            try:
                self.rollback()
            except BaseException as cleanup:
                raise cleanup_failure(primary, cleanup) from cleanup
            raise

    def remove_owned(self):
        observed = os.stat(self.name, dir_fd=self.root_fd, follow_symlinks=False)
        if self.identity is None or (observed.st_dev, observed.st_ino) != (
                self.identity.st_dev, self.identity.st_ino):
            raise PolicyError("PLAYGROUND-CGROUP-REPLACED")
        os.rmdir(self.name, dir_fd=self.root_fd)
        self.name = None

    def rollback(self):
        primary = None
        try:
            if self.name is not None:
                self.remove_owned()
        except BaseException as error:
            primary = error
        finally:
            self.release_handles(primary)

    def release_handles(self, primary=None):
        for attribute in ["fd", "root_fd"]:
            fd = getattr(self, attribute)
            if fd is not None:
                setattr(self, attribute, None)
                try:
                    os.close(fd)
                except BaseException as error:
                    primary = cleanup_failure(primary, error)
        if primary is not None:
            raise primary

    def events(self):
        result = {}
        for name in ["memory.events", "pids.events"]:
            for line in control(self.fd, name).splitlines():
                key, value = line.split()
                result[name + ":" + key] = int(value)
        return result

    def exceeded(self):
        events = self.events()
        return any(events[key] > self.baseline.get(key, 0) for key in
                   ["memory.events:max", "memory.events:oom", "memory.events:oom_kill", "pids.events:max"])

    def populated(self):
        records = control(self.fd, "cgroup.events").splitlines()
        populated = [line for line in records if line.startswith("populated")]
        if len(populated) != 1 or populated[0] not in ["populated 0", "populated 1"]:
            raise PolicyError("PLAYGROUND-CGROUP-EVENTS")
        return populated[0] == "populated 1"

    def kill(self):
        control(self.fd, "cgroup.kill", 1)

    def close(self, reap, deadline):
        if self.closed:
            return
        primary = None
        try:
            self.kill()
            while True:
                children_remain = reap()
                if not self.populated() and not children_remain:
                    break
                if self.clock() >= deadline:
                    raise PolicyError("PLAYGROUND-HOST-TEARDOWN")
                time.sleep(min(0.01, max(0, deadline - self.clock())))
            self.remove_owned()
            self.closed = True
        except BaseException as error:
            primary = error
        finally:
            # Keep failed ownership evidence; never remove a group without proven quiescence.
            self.release_handles(primary)
