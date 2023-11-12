use crate::{GpuProfiler, GpuTimerScope, ProfilerCommandRecorder};

/// Scope that takes ownership of the encoder/pass.
///
/// Calls [`GpuProfiler::end_scope()`] on drop.
pub struct OwningScope<'a, Recorder: ProfilerCommandRecorder> {
    pub profiler: &'a GpuProfiler,
    pub recorder: Recorder,
    pub scope: Option<GpuTimerScope>,
}

impl<'a, W: ProfilerCommandRecorder> OwningScope<'a, W> {
    /// Starts a new profiler scope without nesting.
    #[must_use]
    #[track_caller]
    #[inline]
    pub fn start(
        label: impl Into<String>,
        profiler: &'a GpuProfiler,
        mut recorder: W,
        device: &wgpu::Device,
    ) -> Self {
        let scope = profiler.begin_scope(label, &mut recorder, device, None);
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
        mut recorder: W,
        device: &wgpu::Device,
        parent: Option<&GpuTimerScope>,
    ) -> Self {
        let scope = profiler.begin_scope(label, &mut recorder, device, parent);
        Self {
            profiler,
            recorder,
            scope: Some(scope),
        }
    }
}

impl<'a, R: ProfilerCommandRecorder> super::private::ScopeAccessor<R> for OwningScope<'a, R> {
    fn access(&mut self) -> (&GpuProfiler, &mut R, Option<&GpuTimerScope>) {
        (self.profiler, &mut self.recorder, self.scope.as_ref())
    }
}

impl<'a, R: ProfilerCommandRecorder> std::ops::Deref for OwningScope<'a, R> {
    type Target = R;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.recorder
    }
}

impl<'a, R: ProfilerCommandRecorder> std::ops::DerefMut for OwningScope<'a, R> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.recorder
    }
}

impl<'a, R: ProfilerCommandRecorder> Drop for OwningScope<'a, R> {
    #[inline]
    fn drop(&mut self) {
        if let Some(scope) = self.scope.take() {
            self.profiler.end_scope(&mut self.recorder, scope);
        }
    }
}
