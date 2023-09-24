/// Errors that can occur during [`GpuProfiler::new`].
#[derive(thiserror::Error, Debug)]
pub enum CreationError {
    #[error("GpuProfilerSettings::max_num_pending_frames must be at least 1.")]
    InvalidMaxNumPendingFrames,

    #[cfg(feature = "tracy")]
    #[error("Tracy client doesn't run yet.")]
    TracyClientNotRunning,

    #[cfg(feature = "tracy")]
    #[error("Failed to create Tracy GPU context: {0}")]
    TracyGpuContextCreationError(#[from] tracy_client::GpuContextCreationError),
}

impl PartialEq for CreationError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            #[cfg(feature = "tracy")]
            (Self::TracyGpuContextCreationError(left), Self::TracyGpuContextCreationError(right)) => match left {
                tracy_client::GpuContextCreationError::TooManyContextsCreated => match right {
                    tracy_client::GpuContextCreationError::TooManyContextsCreated => true,
                },
            },
            _ => core::mem::discriminant(self) == core::mem::discriminant(other),
        }
    }
}

/// Errors that can occur during [`GpuProfiler::end_frame`].
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

/// Errors that can occur during [`GpuProfiler::end_scope`].
#[derive(thiserror::Error, Debug, PartialEq, Eq)]
pub enum ScopeError {
    #[error("No profiler GpuProfiler scope was previously opened. For each call to `end_scope` you first need to call `begin_scope`.")]
    NoOpenScope,
}
