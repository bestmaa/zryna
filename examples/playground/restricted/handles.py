"""Directory handles descended without following any path component."""

import os
import pathlib
from errors import PolicyError


def directory(path):
    path = pathlib.PurePosixPath(path)
    if not path.is_absolute() or ".." in path.parts:
        raise PolicyError("PLAYGROUND-DIRECTORY-PATH")
    fd = os.open("/", os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        for part in path.parts[1:]:
            child = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=fd)
            os.close(fd)
            fd = child
        return fd
    except BaseException:
        os.close(fd)
        raise


def regular(root_fd, relative):
    path = pathlib.PurePosixPath(relative)
    if path.is_absolute() or str(path) != relative or ".." in path.parts or not path.parts:
        raise PolicyError("PLAYGROUND-MATERIAL-PATH")
    fd = os.dup(root_fd)
    try:
        for part in path.parts[:-1]:
            child = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=fd)
            os.close(fd)
            fd = child
        return os.open(path.name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=fd)
    finally:
        os.close(fd)


def control(root_fd, name, value=None):
    fd = os.open(name, (os.O_RDONLY if value is None else os.O_WRONLY) |
                 os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=root_fd)
    try:
        if value is not None:
            data = str(value).encode("ascii")
            if os.write(fd, data) != len(data):
                raise PolicyError("PLAYGROUND-CGROUP-WRITE")
            return None
        data = os.read(fd, 65_537)
        if len(data) > 65_536:
            raise PolicyError("PLAYGROUND-CGROUP-READ")
        return data.decode("ascii").strip()
    finally:
        os.close(fd)
