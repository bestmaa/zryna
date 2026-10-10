# Zryna Windows filesystem foundation

This crate owns Zryna's only approved unsafe-Rust exception. It exposes three Windows-only,
handle-relative directory operations on one opaque capability needed by create-only transactions:

- atomic creation of one new directory with an immediately retained authoritative handle;
- no-replace rename of that exact source handle beneath a retained destination parent; and
- empty-directory deletion through that exact owned handle, followed by retained-parent absence
  confirmation.

The creation handle requests `DELETE`, permits read and write sharing, and excludes delete sharing.
The returned `OwnedDirectory` wraps that handle once as a `cap_std::fs::Dir` and retains its parent
and current name through child operations, commit, rollback, and cleanup. No public mutation accepts
an arbitrary file handle. The API accepts one bounded portable ASCII component, never an ambient
path, and provides no path-based fallback or replacement mode.

The command H1 candidate adds a separate read-only exact-file privacy proof for an explicitly
opened grant input. It observes the caller token, regular-file identity, owner and DACL through
the retained file handle. Only the exact caller owner and LocalSystem may receive effective allow
rights; broad inherited grants, Administrators, null DACLs, unfamiliar ACEs and failed observations
reject. A bounded safe parser checks the copied self-relative descriptor. The same handle and
complete descriptor are rechecked after the driver's bounded capture. This candidate does not
activate a public command profile or approve capability contents; the driver must retain no-reparse
ancestors, exclude write/delete sharing, bound the read and dispose of the captured input.

Windows requires every handle to a descendant file or directory to close before an ancestor can be
renamed. This applies even when a descendant was opened with delete sharing. Callers therefore
finish validation while child capabilities and file identities are live, consume those descendants
and every cloned child directory, and only then rename the still-retained exact root capability.
The rename method neither discovers nor closes descendants for the caller.

Removal consumes `OwnedDirectory`, marks the exact empty directory for deletion, closes its handle,
and attempts a no-reparse open of the bound name relative to the retained parent. It returns success
only for an unambiguous not-found result. A foreign replacement or another share-delete handle that
keeps deletion pending returns an error; after marking, that error can outlive this capability until
the other handle closes.

Unsafe operations are confined to the private Windows syscall module. Its invariants are:

1. every raw pointer references a live, correctly aligned and fully initialized ABI buffer;
2. UTF-16 and total buffer lengths are checked before narrowing to Windows integer types;
3. every successful raw handle acquisition is transferred exactly once into RAII ownership;
4. borrowed handles remain live for each synchronous call; and
5. native errors are surfaced after NTSTATUS-to-Win32 mapping, and deletion is not called confirmed
   merely because its authoritative handle accepted a delete disposition.

Focused Windows tests exercise the complete same-handle lifecycle, the required descendant-close
precondition with and without delete sharing, the complete standalone-project handle topology,
destination collisions, hostile pathname mutation, regular-file rejection, deletion-pending
contention, exact 255/256-unit bounds, and failure-path handle close. The repository structure gate
independently enforces canonical lint tables and lexically inspects every Rust source path, including
unreferenced, case-varied, and configuration-disabled files, while excluding comments and literals.
This keeps the exact component manifest and private-module unsafe allowance as the only exception.
The crate grants
no general filesystem authority and has no non-Windows API.
