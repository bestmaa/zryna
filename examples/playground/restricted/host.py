"""Exact host-helper capture after independent toolkit/host-receipt authentication."""

import hashlib
import os
import pathlib
import platform
import re
import stat
import subprocess
import sys
from errors import PolicyError, cleanup_failure, close_descriptors
from handles import directory, regular

OPTIONS = ("--unshare-all", "--unshare-user", "--disable-userns", "--assert-userns-disabled",
           "--clearenv", "--new-session", "--die-with-parent", "--cap-drop", "--proc", "--dev",
           "--size", "--tmpfs", "--dir", "--perms", "--ro-bind-data", "--remount-ro", "--chdir", "--seccomp")

BUBBLEWRAP = {
    "role": "bubblewrap",
    "path": "/usr/local/libexec/zryna-playground-check/bubblewrap-0.12.0/bin/bwrap",
    "bytes": 97_040,
    "sha256": "f64e9068e26f30246f2409b134c4deed8bac88a54af207eff8d81410404e19f8",
    "version": "0.12.0",
}
LIBRARIES = [
    {"path": "/usr/lib/x86_64-linux-gnu/ld-linux-x86-64.so.2", "bytes": 236_616,
     "sha256": "cd4df4f3c7b83673d61189bf2eaebd33ca4f2853ab9772b8a25e025ef99b1e81"},
    {"path": "/usr/lib/x86_64-linux-gnu/libc.so.6", "bytes": 2_125_328,
     "sha256": "8db37cf3f2169f59a0f07ef1fea308c35656668c64c8ff294e1860f4121eb161"},
    {"path": "/usr/lib/x86_64-linux-gnu/libcap.so.2.66", "bytes": 51_536,
     "sha256": "6ac6abc86ac891c6e13486470e26f1d939f47fda9e6b5d5508a7f5ec881adc84"},
]


def validate_helper_receipt(helpers, libraries):
    """Validate selected identities only; this grants no host or executable admission."""
    if not isinstance(helpers, list) or len(helpers) != 2 or \
            any(not isinstance(item, dict) for item in helpers) or \
            {item.get("role") for item in helpers} != {"python", "bubblewrap"}:
        raise PolicyError("PLAYGROUND-HOST-HELPERS")
    if libraries != LIBRARIES:
        raise PolicyError("PLAYGROUND-HOST-LIBRARIES")
    for item in helpers:
        if set(item) != {"role", "path", "bytes", "sha256", "version"} or \
                not isinstance(item["sha256"], str) or not re.fullmatch("[a-f0-9]{64}", item["sha256"]) or \
                type(item["bytes"]) is not int or not 1 <= item["bytes"] <= 134_217_728:
            raise PolicyError("PLAYGROUND-HOST-HELPERS")
        if item["role"] == "bubblewrap":
            if item != BUBBLEWRAP:
                raise PolicyError("PLAYGROUND-HOST-HELPERS")
        elif item["path"] != "/usr/bin/python3.12" or not isinstance(item["version"], str) or \
                not re.fullmatch(r"3\.12\.[0-9]+", item["version"]):
            raise PolicyError("PLAYGROUND-HOST-HELPERS")


class HostCapability:
    """Caller must authenticate expected bytes through the independent signed receipt first."""

    def __init__(self, expected):
        self.files = []
        self.root_fd = None
        try:
            if set(expected) != {"version", "os", "architecture", "kernelRelease", "uid", "helpers", "libraries"}:
                raise PolicyError("PLAYGROUND-HOST-RECEIPT")
            if (expected["version"] != 1 or expected["os"] != {"id": "ubuntu", "versionId": "24.04"}
                    or expected["architecture"] != "x86_64" or type(expected["uid"]) is not int
                    or expected["uid"] <= 0 or os.getuid() != expected["uid"]):
                raise PolicyError("PLAYGROUND-HOST-IDENTITY")
            release = {}
            for line in pathlib.Path("/etc/os-release").read_text().splitlines():
                if "=" in line:
                    key, value = line.split("=", 1)
                    release[key] = value.strip('"')
            if release.get("ID") != "ubuntu" or release.get("VERSION_ID") != "24.04" or \
                    platform.system() != "Linux" or platform.machine() != "x86_64" or \
                    platform.release() != expected["kernelRelease"]:
                raise PolicyError("PLAYGROUND-UNSUPPORTED-HOST")
            helpers = expected["helpers"]
            validate_helper_receipt(helpers, expected["libraries"])
            self.root_fd = directory("/")
            for item in helpers:
                path = item["path"]
                if item["role"] == "bubblewrap":
                    self.bubblewrap = path
                else:
                    if str(pathlib.Path(sys.executable).resolve()) != path or sys.version.split()[0] != item["version"]:
                        raise PolicyError("PLAYGROUND-HOST-HELPERS")
                    self.python = path
                fd = regular(self.root_fd, path[1:])
                self.files.append((fd, item))
                observed = os.fstat(fd)
                if not stat.S_ISREG(observed.st_mode) or observed.st_uid != 0 or \
                        observed.st_mode & (stat.S_IWGRP | stat.S_IWOTH | stat.S_ISUID | stat.S_ISGID):
                    raise PolicyError("PLAYGROUND-HOST-HELPER-MODE")
                if item["role"] == "bubblewrap" and stat.S_IMODE(observed.st_mode) != 0o555:
                    raise PolicyError("PLAYGROUND-HOST-HELPER-MODE")
            for item in expected["libraries"]:
                fd = regular(self.root_fd, item["path"][1:])
                self.files.append((fd, item))
            self.revalidate()
            for arguments, expected_version in [(["--version"], "bubblewrap " +
                                                  next(item["version"] for item in helpers if item["role"] == "bubblewrap")),
                                                 (["--help"], None)]:
                probe = subprocess.run([self.bubblewrap, *arguments], env={}, timeout=2,
                                       stdout=subprocess.PIPE, stderr=subprocess.PIPE)
                if probe.returncode or probe.stderr or len(probe.stdout) > 32_768:
                    raise PolicyError("PLAYGROUND-HOST-HELPER-PROBE")
                text = probe.stdout.decode("utf-8", errors="strict")
                if expected_version and text.strip() != expected_version or \
                        not expected_version and any(option not in text for option in OPTIONS):
                    raise PolicyError("PLAYGROUND-HOST-HELPER-OPTIONS")
            self.revalidate()
        except BaseException as primary:
            try:
                self.close()
            except BaseException as cleanup:
                raise cleanup_failure(primary, cleanup) from cleanup
            raise

    def revalidate(self):
        for fd, item in self.files:
            observed = os.fstat(fd)
            if not stat.S_ISREG(observed.st_mode) or observed.st_uid != 0 or \
                    observed.st_mode & (stat.S_IWGRP | stat.S_IWOTH | stat.S_ISUID | stat.S_ISGID) or \
                    observed.st_size != item["bytes"]:
                raise PolicyError("PLAYGROUND-HOST-HELPER-SUBSTITUTION")
            if item.get("role") == "bubblewrap" and stat.S_IMODE(observed.st_mode) != 0o555:
                raise PolicyError("PLAYGROUND-HOST-HELPER-MODE")
            digest = hashlib.sha256()
            offset = 0
            while offset < item["bytes"]:
                chunk = os.pread(fd, min(65_536, item["bytes"] - offset), offset)
                if not chunk:
                    raise PolicyError("PLAYGROUND-HOST-HELPER-SUBSTITUTION")
                digest.update(chunk)
                offset += len(chunk)
            if digest.hexdigest() != item["sha256"]:
                raise PolicyError("PLAYGROUND-HOST-HELPER-SUBSTITUTION")
            current = regular(self.root_fd, item["path"][1:])
            try:
                actual = os.fstat(current)
                if (observed.st_dev, observed.st_ino) != (actual.st_dev, actual.st_ino):
                    raise PolicyError("PLAYGROUND-HOST-HELPER-SUBSTITUTION")
            finally:
                os.close(current)

    def close(self):
        descriptors = [fd for fd, _ in self.files]
        self.files.clear()
        if self.root_fd is not None:
            descriptors.append(self.root_fd)
            self.root_fd = None
        close_descriptors(descriptors)
