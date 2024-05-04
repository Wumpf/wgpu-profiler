use crate::SettingsError;

/// Settings passed on initialization of [`GpuProfiler`](crate::GpuProfiler).
#[derive(Debug, Clone)]
pub struct GpuProfilerSettings {
    /// Enables/disables gpu timer queries.
    ///
    /// If false, the profiler will not emit any timer queries, making most operations on [`GpuProfiler`](crate::GpuProfiler) no-ops.
    ///
    /// Since all resource creation is done lazily, this provides an effective way of disabling the profiler at runtime
    /// without the need of special build configurations or code to handle enabled/disabled profiling.
    pub enable_timer_queries: bool,

    /// Enables/disables debug markers for all scopes on the respective encoder or pass.
    ///
    /// This is useful for debugging with tools like [RenderDoc](https://renderdoc.org/).
    /// Debug markers will be emitted even if the device does not support timer queries or disables them via
    /// [`GpuProfilerSettings::enable_timer_queries`].
    pub enable_debug_groups: bool,

    /// The profiler queues up to `max_num_pending_frames` "profiler-frames" at a time.
    ///
    /// A profiler-frame is regarded as in-flight until its queries have been successfully
    /// resolved using [`GpuProfiler::process_finished_frame`](crate::GpuProfiler::process_finished_frame).
    /// How long this takes to happen, depends on how fast buffer mappings return successfully
    /// which in turn primarily depends on how fast the device is able to finish work queued to the [`wgpu::Queue`].
    ///
    /// If this threshold is exceeded, [`GpuProfiler::end_frame`](crate::GpuProfiler::end_frame) will silently drop frames.
    /// *Newer* frames will be dropped first in order to get results back eventually.
    /// (If the profiler were to drop the oldest frame, one may end up in a situation where there is never
    /// frame that is fully processed and thus never any results to be retrieved).
    ///
    /// Good values for `max_num_pending_frames` are 2-4 but may depend on your application workload
    /// and GPU-CPU syncing strategy.
    /// Must be greater than 0.
    pub max_num_pending_frames: usize,
}

impl Default for GpuProfilerSettings {
    fn default() -> Self {
        Self {
            enable_timer_queries: true,
            enable_debug_groups: true,
            max_num_pending_frames: 3,
        }
    }
}

impl GpuProfilerSettings {
    pub fn validate(&self) -> Result<(), SettingsError> {
        if self.max_num_pending_frames == 0 {
            Err(SettingsError::InvalidMaxNumPendingFrames)
        } else {
            Ok(())
        }
    }
}
