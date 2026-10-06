"""Frozen independent case selection; no producer, runtime admission or image authority.

Parser units use private mock bindings and cannot authenticate an initial image.
Real cases require an independently retained exact-source build and original image.
A rejected descriptor mutation under that image exercises its digest gate only.
"""
import json
import re

FORMAT = 'zryna.native-installation-internal.v1'
REPOSITORY = 'https://github.com/zryna/zryna'
DESCRIPTOR_PATH = 'metadata/native-provider.json'
MARKER_PREFIX = b'ZRYNA-NATIVE-INSTALLATION-INTERNAL-V1\0'
COMPILE_BINDINGS = (
    'ZRYNA_PRIVATE_NATIVE_INSTALLATION_SHA256',
    'ZRYNA_PRIVATE_NATIVE_SOURCE_COMMIT',
    'ZRYNA_PRIVATE_NATIVE_SOURCE_TREE',
)
TARGETS = {
    'linux': ('x86_64-unknown-linux-gnu', 'bin/native-installation-proof'),
    'win32': ('x86_64-pc-windows-msvc', 'bin/native-installation-proof.exe'),
}
PRIVATE_MODES = {'descriptor': 0o600, 'license': 0o600, 'executable': 0o700}
# These are purpose-private creation modes, not public distribution0644/0755.
# Windows declares these modes under the existing mode_matches policy; no ACL claim.

LIMITS = {'descriptor_bytes': 4096, 'license_bytes': 65536,
          'executable_bytes': 128 * 1024 * 1024, 'JSON_depth': 12}


def canonical_bytes(value):
    """Exact sorted-key, compact UTF8 JSON with one final LF; encoder only."""
    return (json.dumps(value, sort_keys=True, separators=(',', ':'),
                       ensure_ascii=False, allow_nan=False) + '\n').encode('utf8')


def descriptor(version, commit, tree, platform, license_sha256):
    for value, size in ((commit, 40), (tree, 40), (license_sha256, 64)):
        if type(value) is not str or not re.fullmatch('[a-f0-9]{' + str(size) + '}', value):
            raise ValueError('oracle inputs require exact lowercase hexadecimal')
    if type(version) is not str or not version:
        raise ValueError('oracle version must be the actual Cargo package version')
    target, cli = TARGETS[platform]
    return {'format': FORMAT, 'version': version,
            'source': {'repository': REPOSITORY, 'commit': commit, 'tree': tree},
            'target': target, 'cli': cli, 'protocols': [2, 3, 4],
            'license_sha256': license_sha256}


PARSER_CASES = (
    'canonical-current-host-positive', 'duplicate-key-rejected',
    'unknown-top-field-rejected', 'unknown-source-field-rejected',
    'missing-field-rejected', 'JSON-whitespace-rejected',
    'JSON-key-order-rejected', 'missing-final-LF-rejected',
    'extra-final-LF-rejected', 'UTF8-BOM-rejected', 'invalid-UTF8-rejected',
    'trailing-document-rejected', 'escaped-equivalent-string-rejected',
    'JSON-depth-13-rejected', 'descriptor-byte-4097-rejected',
    'foreign-format-rejected', 'foreign-version-rejected',
    'foreign-repository-rejected', 'wrong-commit-rejected',
    'wrong-tree-rejected', 'foreign-host-target-rejected',
    'wrong-cli-name-rejected', 'traversing-cli-path-rejected',
    'protocol-missing-rejected', 'protocol-duplicate-rejected',
    'protocol-reordered-rejected', 'protocol-unknown-rejected',
    'protocol-bool-rejected', 'protocol-float-rejected',
    'license-digest-uppercase-rejected', 'license-digest-length-rejected',
)

POSITIVE_CASES = (
    {'id': 'real-native-v2-package', 'protocol': 2, 'expected': 'verified-summary'},
    {'id': 'real-native-v3-import-package', 'protocol': 3, 'expected': 'verified-summary'},
    {'id': 'real-native-v4-ownership-package', 'protocol': 4, 'expected': 'verified-summary'},
    {'id': 'relocated-complete-installation', 'protocols': [2, 3, 4],
     'expected': 'verified-summary', 'initial_auth': 'same original image bytes'},
    {'id': 'runtime-env-cannot-override-compiled-authority', 'protocols': [2, 3, 4],
     'expected': 'verified-summary', 'mutation': 'set all three compile-binding envs to foreign values'},
)

# Every hostile probe attempt must first establish a valid original baseline.
# Effective mutation and OS-denied prevention are separate recorded dispositions.
REAL_CASES = (
    ('descriptor-byte-change', 'pre-capture', 'compiled-descriptor-digest'),
    ('descriptor-duplicate-key', 'pre-capture', 'compiled-descriptor-digest'),
    ('descriptor-unknown-field', 'pre-capture', 'compiled-descriptor-digest'),
    ('descriptor-foreign-source', 'pre-capture', 'compiled-descriptor-digest'),
    ('license-byte-change', 'pre-capture', 'license-digest'),
    ('descriptor-missing', 'pre-capture', 'installation-topology'),
    ('license-missing', 'pre-capture', 'installation-topology'),
    ('extra-root-file', 'pre-capture', 'installation-inventory'),
    ('extra-root-empty-directory', 'pre-capture', 'installation-inventory'),
    ('extra-bin-file', 'pre-capture', 'installation-inventory'),
    ('extra-metadata-file', 'pre-capture', 'installation-inventory'),
    ('descriptor-hardlink', 'pre-capture', 'installation-single-link'),
    ('license-hardlink', 'pre-capture', 'installation-single-link'),
    ('executable-hardlink', 'pre-capture', 'installation-single-link'),
    ('descriptor-symlink', 'pre-capture', 'installation-no-links'),
    ('license-symlink', 'pre-capture', 'installation-no-links'),
    ('metadata-directory-symlink', 'pre-capture', 'installation-no-links'),
    ('installation-parent-link', 'pre-capture', 'installation-no-links'),
    ('descriptor-oversize', 'pre-capture', 'descriptor-bound'),
    ('license-oversize', 'pre-capture', 'license-bound'),
    ('descriptor-replaced-identical-bytes', 'after-capture', 'retained-installation-identity'),
    ('license-replaced-identical-bytes', 'after-capture', 'retained-installation-identity'),
    ('executable-path-replaced-identical-bytes', 'after-capture', 'retained-image-identity'),
    ('installation-parent-replaced-identical-tree', 'after-capture', 'retained-ancestor-identity'),
    ('extra-file-after-capture', 'after-capture', 'retained-installation-inventory'),
    ('source-replaced-identical-bytes', 'after-source-capture', 'retained-source-identity'),
    ('source-byte-change', 'after-source-capture', 'retained-source-bytes'),
    ('source-parent-replaced', 'after-source-capture', 'retained-source-ancestor'),
    ('package-manifest-replaced-identical-bytes', 'after-source-capture', 'retained-package-identity'),
    ('package-lock-byte-change', 'after-source-capture', 'frozen-package-lock'),
    ('source-symlink-substitution', 'after-source-capture', 'retained-source-no-links'),
)

OBSERVATION_CASES = (
    {'id': 'source-extra-file', 'phase': 'after-source-capture',
     'expected': 'record actual owning-contract disposition',
     'limit': 'No stronger unrelated-file census is inferred from existing CapturedProject; no protected source changes to force denial.'},
)

EXTERNAL_IMAGE_CASES = (
    {'id': 'missing-private-marker', 'expected': 'reject-before-execution',
     'authority': 'independent original build/image inspector; never execute a forged image'},
    {'id': 'foreign-purpose-marker', 'expected': 'reject-before-execution',
     'authority': 'independent original build/image inspector'},
    {'id': 'modified-executable-bytes', 'expected': 'reject-before-execution',
     'authority': 'retained authenticated actual build executable SHA'},
    {'id': 'unprepared-real-build', 'expected': 'capture-current-rejected',
     'authority': 'separate real build without private compile-binding envs'},
    {'id': 'running-image-versus-installed-path', 'expected': 'reject-if-effective',
     'authority': 'actual retained image/path identities; OS-denied attempts recorded separately',
     'Windows_limit': 'existing running_identity reopens current_exe; no Linux proc-image transfer'},
)

FAILURE_RULES = {
    'effective_mutation': 'reject before verified syntax/closure access or successful summary',
    'OS_denied_mutation': 'record actual OS denial + unchanged bytes; no exercised rejection credit',
    'unsupported_mutation_setup': 'blocked obligation, never passed or ignored',
    'each_hostile_baseline': 'original exact image/descriptor/license/package positive first',
    'syntax_failure': 'no source-after-error claim under consuming existing native verification API',
    'installation_initial_auth': 'actual compiled image + source/marker binding independently verified',
    'artifact_byte_capture': 'retain exact executed argv/exit/stdout/stderr, before/after bytes and identities',
    'noNode': 'private probe environment PATH empty and Node/pnpm/runtime hooks absent; no ordinary-default credit',
    'no_permission_change': 'ambient0077 natural metadata/LICENSE0600 and original compiled image0700; no chmod, umask, credential or security-setting changes',
}
