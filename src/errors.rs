/// Errors that can occur during profiler creation.
#[cfg_attr(not(feature = "tracy"), derive(PartialEq))]
#[derive(thiserror::Error, Debug)]
pub enum CreationError {
    #[error(transparent)]
    InvalidSettings(#[from] SettingsError),

    #[cfg(feature = "tracy")]
    #[error("Tracy client doesn't run yet.")]
    TracyClientNotRunning,

    #[cfg(feature = "tracy")]
    #[error("Failed to create Tracy GPU context: {0}")]
    TracyGpuContextCreationError(#[from] tracy_client::GpuContextCreationError),
}

#[cfg(feature = "tracy")]
impl PartialEq for CreationError {
    fn eq(&self, other: &Self) -> bool {
        match self {
            CreationError::InvalidSettings(left) => match other {
                CreationError::InvalidSettings(right) => left == right,
                _ => false,
            },
            CreationError::TracyClientNotRunning => matches!(other, CreationError::TracyClientNotRunning),
            CreationError::TracyGpuContextCreationError(left) => match left {
                tracy_client::GpuContextCreationError::TooManyContextsCreated => matches!(
                    other,
                    CreationError::TracyGpuContextCreationError(tracy_client::GpuContextCreationError::TooManyContextsCreated)
                ),
            },
        }
    }
}

impl Eq for CreationError {}

/// Errors that can occur during settings validation and change.
#[derive(thiserror::Error, Debug, PartialEq, Eq)]
pub enum SettingsError {
    #[error("GpuProfilerSettings::max_num_pending_frames must be at least 1.")]
    InvalidMaxNumPendingFrames,

    #[error("Can't change settings while there's open profiling scopes.")]
    HasOpenScopes,
}

/// Errors that can occur during [`crate::GpuProfiler::end_frame`].
#[derive(thiserror::Error, Debug, PartialEq, Eq)]
pub enum EndFrameError {
    #[error("All profiling scopes need to be closed before ending a frame. The following scopes were still open: {0:?}")]
    UnclosedScopes(Vec<String>),

    #[error(
        "Not all queries were resolved before ending a frame.\n
Call `GpuProfiler::resolve_queries` after all profiling scopes have been closed and before ending the frame.\n
There were still {0} queries unresolved"
    )]
    UnresolvedQueries(u32),
}

/// Errors that can occur during [`crate::GpuProfiler::end_scope`].
#[derive(thiserror::Error, Debug, PartialEq, Eq)]
pub enum ScopeError {
    #[error("No profiler GpuProfiler scope was previously opened. For each call to `end_scope` you first need to call `begin_scope`.")]
    NoOpenScope,
}
