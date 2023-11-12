use crate::{GpuProfiler, GpuTimerScope, ProfilerCommandRecorder};

use super::private::ScopeAccessor;

/// Scope that takes a (mutable) reference to the encoder/pass.
///
/// Calls [`GpuProfiler::end_scope()`] on drop.
pub struct Scope<'a, Recorder: ProfilerCommandRecorder> {
    pub profiler: &'a GpuProfiler,
    pub recorder: &'a mut Recorder,
    pub scope: Option<GpuTimerScope>,
}

impl<'a, W: ProfilerCommandRecorder> Scope<'a, W> {
    /// Starts a new profiler scope without nesting.
    #[must_use]
    #[track_caller]
    #[inline]
    pub fn start(
        label: impl Into<String>,
        profiler: &'a GpuProfiler,
        recorder: &'a mut W,
        device: &wgpu::Device,
    ) -> Self {
        let scope = profiler.begin_scope(label, recorder, device, None);
        Self {
            profiler,
            recorder,
            scope: Some(scope),
        }
    }

    /// Starts a new profiler scope nested in another scope.
    #[must_use]
    #[track_caller]
    #[inline]
    pub fn start_nested(
        label: impl Into<String>,
        profiler: &'a GpuProfiler,
        recorder: &'a mut W,
        device: &wgpu::Device,
        parent: Option<&GpuTimerScope>,
    ) -> Self {
        let scope = profiler.begin_scope(label, recorder, device, parent);
        Self {
            profiler,
            recorder,
            scope: Some(scope),
        }
    }
}

impl<'a, Recorder: ProfilerCommandRecorder> ScopeAccessor<Recorder> for Scope<'a, Recorder> {
    fn access(&mut self) -> (&GpuProfiler, &mut Recorder, Option<&GpuTimerScope>) {
        (self.profiler, self.recorder, self.scope.as_ref())
    }
}

impl<'a, W: ProfilerCommandRecorder> std::ops::Deref for Scope<'a, W> {
    type Target = W;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.recorder
    }
}

impl<'a, W: ProfilerCommandRecorder> std::ops::DerefMut for Scope<'a, W> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.recorder
    }
}

impl<'a, R: ProfilerCommandRecorder> Drop for Scope<'a, R> {
    #[inline]
    fn drop(&mut self) {
        if let Some(scope) = self.scope.take() {
            self.profiler.end_scope(self.recorder, scope);
        }
    }
}
