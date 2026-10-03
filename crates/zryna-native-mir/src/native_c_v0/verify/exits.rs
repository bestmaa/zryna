//! Independent finite terminal machine replay; release failure has no retry or suffix action.

use super::super::{MirError, raw, require};
use zryna_native_c_ir::{
    VerifiedFunction,
    contract::{BoundaryExitKind, FailureRoute},
};

pub(super) fn check(
    function: &raw::Function,
    source: VerifiedFunction<'_>,
) -> Result<(), MirError> {
    for (claim, original) in function.effects.iter().zip(source.effects()) {
        require(
            claim.exit_instructions.len() == original.exits().len(),
            "ZRYNA-C4106",
            "mir-complete-terminal-inventory",
        )?;
        for (actions, exit) in claim.exit_instructions.iter().zip(original.exits()) {
            // Each original loan contributes one action and each drop two, plus one terminal.
            // Reject raw length amplification before iteration or any allocation from claims.
            let expected_len = exit
                .cleanup
                .len()
                .checked_mul(2)
                .and_then(|drops| drops.checked_add(exit.end_loans.len()))
                .and_then(|prefix| prefix.checked_add(1))
                .ok_or_else(|| MirError::new("ZRYNA-C4107", "mir-exit-size"))?;
            require(actions.len() == expected_len, "ZRYNA-C4106", "mir-terminal-actions")?;
            let mut index = 0;
            for loan in &exit.end_loans {
                require(
                    actions[index] == raw::ExitInstruction::EndLoan(*loan),
                    "ZRYNA-C4106",
                    "mir-end-loans-before-drops",
                )?;
                index += 1;
            }
            for (drop, requirement) in exit.cleanup.iter().enumerate() {
                require(
                    actions[index]
                        == raw::ExitInstruction::Release { drop, owner: requirement.owner() }
                        && actions[index + 1] == raw::ExitInstruction::StopOnReleaseFailure,
                    "ZRYNA-C4106",
                    "mir-reverse-release-stop",
                )?;
                index += 2;
            }
            require(
                actions[index] == raw::ExitInstruction::Finish,
                "ZRYNA-C4106",
                "mir-transfer-after-cleanup",
            )?;
            require(
                exit.release_failure_route == FailureRoute::ReleaseFailureOverridesUnresolved,
                "ZRYNA-C4105",
                "mir-release-failure-override",
            )?;
            if !exit.cleanup_required {
                require(
                    exit.cleanup.is_empty()
                        && exit.end_loans.is_empty()
                        && matches!(
                            exit.kind,
                            BoundaryExitKind::ForeignFailure(
                                FailureRoute::ProcessFailureNoCleanupGuarantee
                            )
                        ),
                    "ZRYNA-C4105",
                    "mir-process-no-cleanup-promise",
                )?;
            }
        }
    }
    Ok(())
}
