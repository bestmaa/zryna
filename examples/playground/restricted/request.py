"""Closed wire admission before acquiring any provider material or launching a child."""

import json

from cgroup import PolicyError


def structural_budget(data):
    stack = []
    quoted = escaped = False
    work = 0
    for byte in data:
        if quoted:
            if escaped:
                escaped = False
            elif byte == 92:
                escaped = True
            elif byte == 34:
                quoted = False
            continue
        if byte == 34:
            quoted = True
        elif byte in (123, 91):
            stack.append(1)
            if len(stack) > 8:
                raise PolicyError("PLAYGROUND-REQUEST-STRUCTURE")
        elif byte in (125, 93):
            if stack:
                stack.pop()
        elif byte == 44 and stack:
            stack[-1] += 1
            if stack[-1] > 512:
                raise PolicyError("PLAYGROUND-REQUEST-STRUCTURE")
        if byte not in (32, 9, 10, 13):
            work += 1
            if work > 16_384:
                raise PolicyError("PLAYGROUND-REQUEST-STRUCTURE")


def validate_source_request(data):
    if not isinstance(data, bytes) or len(data) > 32_768:
        raise PolicyError("PLAYGROUND-REQUEST-LIMIT")
    structural_budget(data)

    def pairs(entries):
        result = {}
        for key, value in entries:
            if key in result:
                raise ValueError("duplicate key")
            result[key] = value
        return result

    def invalid_constant(_value):
        raise ValueError("non-JSON number")

    try:
        value = json.loads(data.decode("utf-8"), object_pairs_hook=pairs,
                           parse_constant=invalid_constant, parse_float=invalid_constant)
        if type(value) is not dict or set(value) != {"version", "revision", "source"}:
            raise ValueError("shape")
        if type(value["version"]) is not int or value["version"] != 1:
            raise ValueError("version")
        if type(value["revision"]) is not int or not 1 <= value["revision"] <= 9_007_199_254_740_991:
            raise ValueError("revision")
        if type(value["source"]) is not str or len(value["source"].encode("utf-8")) > 4096:
            raise ValueError("source")
    except (ValueError, UnicodeError, RecursionError) as error:
        raise PolicyError("PLAYGROUND-REQUEST") from error
    return value
