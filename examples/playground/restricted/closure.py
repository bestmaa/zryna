"""Finite independently authenticated files, captured as sealed Linux memfds."""

import fcntl
import hashlib
import os
import pathlib
import stat

from cgroup import PolicyError
from errors import cleanup_failure, close_descriptors
from handles import directory, regular


class CapturedClosure:
    def __init__(self, root, entries):
        self.fds = []
        self.mounts = []
        self.root_fd = None
        self.root = pathlib.Path(root)
        if not self.root.is_absolute() or self.root.resolve() != self.root:
            raise PolicyError("PLAYGROUND-MATERIAL-ROOT")
        if not isinstance(entries, list) or not 1 <= len(entries) <= 128:
            raise PolicyError("PLAYGROUND-MATERIAL-INVENTORY")
        total = 0
        destinations = set()
        try:
            self.root_fd = directory(str(self.root))
            for entry in entries:
                if set(entry) != {"path", "mount", "bytes", "sha256", "executable"}:
                    raise PolicyError("PLAYGROUND-MATERIAL-INVENTORY")
                relative = pathlib.PurePosixPath(entry["path"])
                destination = pathlib.PurePosixPath(entry["mount"])
                if relative.is_absolute() or str(relative) != entry["path"] or ".." in relative.parts:
                    raise PolicyError("PLAYGROUND-MATERIAL-PATH")
                if (str(destination) != entry["mount"] or ".." in destination.parts
                        or not destination.is_absolute() or destination in destinations):
                    raise PolicyError("PLAYGROUND-MATERIAL-PATH")
                if not (str(destination) == "/app/compiler" or str(destination).startswith(
                        ("/materials/", "/lib/x86_64-linux-gnu/", "/lib64/"))):
                    raise PolicyError("PLAYGROUND-MATERIAL-PATH")
                if type(entry["bytes"]) is not int or not 1 <= entry["bytes"] <= 268_435_456:
                    raise PolicyError("PLAYGROUND-MATERIAL-BYTES")
                total += entry["bytes"]
                if total > 536_870_912 or type(entry["executable"]) is not bool:
                    raise PolicyError("PLAYGROUND-MATERIAL-BYTES")
                destinations.add(destination)
                self.capture(entry)
            required = {pathlib.PurePosixPath("/app/compiler"),
                        pathlib.PurePosixPath("/materials/runtime/node/bin/node")}
            if not required <= destinations:
                raise PolicyError("PLAYGROUND-MATERIAL-INVENTORY")
        except BaseException as primary:
            try:
                self.close()
            except BaseException as cleanup:
                raise cleanup_failure(primary, cleanup) from cleanup
            raise

    def capture(self, entry):
        source = regular(self.root_fd, entry["path"])
        digest = hashlib.sha256()
        size = 0
        primary = None
        try:
            retained = os.memfd_create("playground-material", os.MFD_CLOEXEC | os.MFD_ALLOW_SEALING)
            self.fds.append(retained)
            observed = os.fstat(source)
            if not stat.S_ISREG(observed.st_mode) or observed.st_size != entry["bytes"]:
                raise PolicyError("PLAYGROUND-MATERIAL-TYPE")
            while True:
                chunk = os.read(source, 65_536)
                if not chunk:
                    break
                size += len(chunk)
                if size > entry["bytes"]:
                    raise PolicyError("PLAYGROUND-MATERIAL-BYTES")
                digest.update(chunk)
                view = memoryview(chunk)
                while view:
                    written = os.write(retained, view)
                    if written <= 0:
                        raise PolicyError("PLAYGROUND-MATERIAL-WRITE")
                    view = view[written:]
            if size != entry["bytes"] or digest.hexdigest() != entry["sha256"]:
                raise PolicyError("PLAYGROUND-MATERIAL-HASH")
            after = os.fstat(source)
            fields = ("st_dev", "st_ino", "st_size", "st_mtime_ns", "st_ctime_ns")
            if any(getattr(observed, field) != getattr(after, field) for field in fields):
                raise PolicyError("PLAYGROUND-MATERIAL-CHANGED")
            os.fchmod(retained, 0o555 if entry["executable"] else 0o444)
            fcntl.fcntl(retained, fcntl.F_ADD_SEALS, fcntl.F_SEAL_WRITE | fcntl.F_SEAL_GROW
                        | fcntl.F_SEAL_SHRINK | fcntl.F_SEAL_SEAL)
            os.lseek(retained, 0, os.SEEK_SET)
            self.mounts.append((retained, entry["mount"], "0555" if entry["executable"] else "0444"))
        except BaseException as error:
            primary = error
        try:
            os.close(source)
        except BaseException as cleanup:
            raise cleanup_failure(primary, cleanup) from cleanup
        if primary is not None:
            raise primary

    def arguments(self):
        directories = set()
        for _, destination, _ in self.mounts:
            directories.update(str(parent) for parent in pathlib.PurePosixPath(destination).parents
                               if str(parent) != "/")
        arguments = []
        for directory in sorted(directories, key=lambda value: (value.count("/"), value)):
            arguments += ["--dir", directory]
        for fd, destination, mode in self.mounts:
            os.lseek(fd, 0, os.SEEK_SET)
            arguments += ["--perms", mode, "--ro-bind-data", str(fd), destination]
        return arguments

    def close(self):
        descriptors, self.fds = self.fds, []
        if self.root_fd is not None:
            descriptors.append(self.root_fd)
            self.root_fd = None
        close_descriptors(descriptors)
