//! Distinct material-staging and runtime limits within an already isolated process.

use super::RestrictedBrowserError;

/// Applies and verifies the fixed runtime limits after authentic material staging.
///
/// The external bootstrap needs larger limits while copying authenticated material files.
/// This irreversible operation belongs only in the dedicated compiler process.
///
/// # Errors
/// Rejects unsupported hosts, unavailable limits, or failed limit readback.
pub fn restrict_browser_runtime() -> Result<(), RestrictedBrowserError> {
    #[cfg(target_os = "linux")]
    {
        use nix::sys::resource::{Resource, getrlimit, setrlimit};

        for (resource, limit) in [
            (Resource::RLIMIT_CORE, 0),
            (Resource::RLIMIT_FSIZE, 1_277_956),
            (Resource::RLIMIT_NOFILE, 128),
        ] {
            setrlimit(resource, limit, limit)
                .map_err(|_| RestrictedBrowserError("PLAYGROUND-RUNTIME-LIMIT"))?;
            if getrlimit(resource)
                .map_err(|_| RestrictedBrowserError("PLAYGROUND-RUNTIME-LIMIT"))?
                != (limit, limit)
            {
                return Err(RestrictedBrowserError("PLAYGROUND-RUNTIME-LIMIT"));
            }
        }
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    {
        Err(RestrictedBrowserError("PLAYGROUND-UNSUPPORTED-HOST"))
    }
}

pub(super) fn restrict_material_setup() -> Result<(), RestrictedBrowserError> {
    #[cfg(target_os = "linux")]
    {
        use nix::sys::resource::{getrlimit, setrlimit};
        material_limits(
            |resource| {
                getrlimit(resource).map_err(|_| RestrictedBrowserError("PLAYGROUND-RUNTIME-LIMIT"))
            },
            |resource, value| {
                setrlimit(resource, value, value)
                    .map_err(|_| RestrictedBrowserError("PLAYGROUND-RUNTIME-LIMIT"))
            },
        )
    }
    #[cfg(not(target_os = "linux"))]
    {
        Err(RestrictedBrowserError("PLAYGROUND-UNSUPPORTED-HOST"))
    }
}

#[cfg(target_os = "linux")]
fn material_limits(
    mut read: impl FnMut(
        nix::sys::resource::Resource,
    ) -> Result<
        (nix::sys::resource::rlim_t, nix::sys::resource::rlim_t),
        RestrictedBrowserError,
    >,
    mut set: impl FnMut(
        nix::sys::resource::Resource,
        nix::sys::resource::rlim_t,
    ) -> Result<(), RestrictedBrowserError>,
) -> Result<(), RestrictedBrowserError> {
    use nix::sys::resource::Resource;
    for (resource, expected) in [
        (Resource::RLIMIT_CORE, 0),
        (Resource::RLIMIT_FSIZE, 268_435_456),
        (Resource::RLIMIT_NOFILE, 512),
    ] {
        if read(resource)? != (expected, expected) {
            return Err(RestrictedBrowserError("PLAYGROUND-RUNTIME-LIMIT"));
        }
    }
    set(Resource::RLIMIT_FSIZE, 12_582_912)?;
    if read(Resource::RLIMIT_FSIZE)? != (12_582_912, 12_582_912) {
        return Err(RestrictedBrowserError("PLAYGROUND-RUNTIME-LIMIT"));
    }
    Ok(())
}

#[cfg(all(test, target_os = "linux"))]
mod material_tests {
    use super::{RestrictedBrowserError, material_limits};
    use nix::sys::resource::Resource;
    use std::cell::Cell;

    fn inherited(resource: Resource) -> (u64, u64) {
        let value = match resource {
            Resource::RLIMIT_CORE => 0,
            Resource::RLIMIT_FSIZE => 268_435_456,
            Resource::RLIMIT_NOFILE => 512,
            _ => panic!("unexpected material resource"),
        };
        (value, value)
    }

    #[test]
    fn setup_requires_exact_soft_and_hard_limits_before_any_tightening() {
        for resource in [Resource::RLIMIT_CORE, Resource::RLIMIT_FSIZE, Resource::RLIMIT_NOFILE] {
            for wrong in [(0, u64::MAX), (u64::MAX, u64::MAX), (1, 1), (1_048_576, 1_048_576)] {
                let writes = Cell::new(0);
                assert!(
                    material_limits(
                        |selected| Ok(if selected == resource {
                            wrong
                        } else {
                            inherited(selected)
                        }),
                        |_, _| {
                            writes.set(writes.get() + 1);
                            Ok(())
                        },
                    )
                    .is_err()
                );
                assert_eq!(writes.get(), 0, "incorrect inheritance must not attempt an increase");
            }
        }
    }

    #[test]
    fn setup_tightens_only_file_size_and_requires_exact_readback() {
        let tightened = Cell::new(false);
        material_limits(
            |selected| {
                Ok(if selected == Resource::RLIMIT_FSIZE && tightened.get() {
                    (12_582_912, 12_582_912)
                } else {
                    inherited(selected)
                })
            },
            |selected, value| {
                assert_eq!(selected, Resource::RLIMIT_FSIZE);
                assert_eq!(value, 12_582_912);
                tightened.set(true);
                Ok(())
            },
        )
        .expect("exact bounded material phase");
        assert!(tightened.get());
        assert!(material_limits(|selected| Ok(inherited(selected)), |_, _| Ok(())).is_err());
    }

    #[test]
    fn setup_read_and_write_failures_stop_the_phase() {
        let reads = Cell::new(0);
        for failing_read in 0..4 {
            reads.set(0);
            assert!(
                material_limits(
                    |selected| {
                        let index = reads.get();
                        reads.set(index + 1);
                        if index == failing_read {
                            Err(RestrictedBrowserError("PLAYGROUND-RUNTIME-LIMIT"))
                        } else {
                            Ok(inherited(selected))
                        }
                    },
                    |_, _| Ok(()),
                )
                .is_err()
            );
            assert_eq!(reads.get(), failing_read + 1);
        }
        assert!(
            material_limits(
                |selected| Ok(inherited(selected)),
                |_, _| Err(RestrictedBrowserError("PLAYGROUND-RUNTIME-LIMIT"))
            )
            .is_err()
        );
    }
}
