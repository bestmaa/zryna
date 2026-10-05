"""Versioned Linux x86-64 compiler-only syscall allowlist, requiring positive host proof."""

import platform
import struct

from cgroup import PolicyError

# The sandbox has only fixed read-only executable/material/library files and private scratch.
# No socket, mount, ptrace, BPF, module, keyring, namespace or process-memory authority.
ALLOW = (0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 19, 20,
         21, 22, 23, 24, 25, 28, 32, 33, 35, 36, 37, 38, 39, 40, 56, 57, 58, 59,
         60, 61, 62, 63, 72, 73, 74, 75, 77, 78, 79, 80, 81, 82, 83, 84, 85,
         86, 87, 89, 90, 91, 95, 96, 97, 98, 99, 100, 102, 104, 107, 108,
         110, 111, 112, 131, 137, 138, 157, 158, 186, 187, 201, 202, 203, 204,
         213, 217, 218, 228, 229, 230, 231, 232, 233, 234, 247, 257, 262, 263,
         267, 268, 269, 270, 271, 273, 275, 276, 278, 281, 282, 284, 289,
         290, 291, 292, 293, 302, 318, 319, 324, 332, 334, 436)


def compiler_filter():
    if platform.system() != "Linux" or platform.machine() != "x86_64":
        raise PolicyError("PLAYGROUND-SECCOMP-PLATFORM")
    # sock_filter(code, jt, jf, k): audit architecture is checked before syscall number.
    instructions = [(0x20, 0, 0, 4), (0x15, 1, 0, 0xC000003E),
                    (0x06, 0, 0, 0x80000000), (0x20, 0, 0, 0)]
    for number in ALLOW:
        instructions += [(0x15, 0, 1, number), (0x06, 0, 0, 0x7FFF0000)]
    # clone3 returns ENOSYS so libc/Node use the admitted clone fallback.
    instructions += [(0x15, 0, 1, 435), (0x06, 0, 0, 0x00050026),
                     (0x06, 0, 0, 0x80000000)]
    return b"".join(struct.pack("HBBI", *instruction) for instruction in instructions)
