use crate::{GpuProfiler, GpuTimerScope, ProfilerCommandRecorder};

/// Scope that takes ownership of the encoder/pass.
///
/// Does NOT call [`GpuProfiler::end_scope()`] on drop.
/// This construct is just for completeness in cases where working with scopes is preferred but one can't rely on the Drop call in the right place.
/// This is useful when the owned value needs to be recovered after the end of the scope.
/// In particular, to submit a [`wgpu::CommandEncoder`] to a queue, ownership of the encoder is necessary.
pub struct ManualOwningScope<'a, Recorder: ProfilerCommandRecorder> {
    pub profiler: &'a GpuProfiler,
    pub recorder: Recorder,
    pub scope: Option<GpuTimerScope>,
}

impl<'a, R: ProfilerCommandRecorder> ManualOwningScope<'a, R> {
    /// Starts a new profiler scope.
    ///
    /// Scope is NOT closed on drop and needs to be closed manually with [`ManualOwningScope::end_scope`]
    #[must_use]
    #[track_caller]
    #[inline]
    pub fn start(
        label: impl Into<String>,
        profiler: &'a GpuProfiler,
        mut recorder: R,
        device: &wgpu::Device,
    ) -> Self {
        let scope = profiler.begin_scope(label, &mut recorder, device, None);
        Self {
            profiler,
            recorder,
            scope: Some(scope),
        }
    }

    /// Starts a new profiler scope nested in another one.
    ///
    /// Scope is NOT closed on drop and needs to be closed manually with [`ManualOwningScope::end_scope`]
    #[must_use]
    #[track_caller]
    #[inline]
    pub fn start_nested(
        label: impl Into<String>,
        profiler: &'a GpuProfiler,
        mut recorder: R,
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

    /// Ends the scope allowing the extraction of the owned [`ProfilerCommandRecorder`].
    #[track_caller]
    #[inline]
    pub fn end_scope(mut self) -> R {
        // Can't fail since creation implies begin_scope.
        self.profiler
            .end_scope(&mut self.recorder, self.scope.take().unwrap());
        self.recorder
    }
}

impl<'a, R: ProfilerCommandRecorder> super::private::ScopeAccessor<R> for ManualOwningScope<'a, R> {
    fn access(&mut self) -> (&GpuProfiler, &mut R, Option<&GpuTimerScope>) {
        (self.profiler, &mut self.recorder, self.scope.as_ref())
    }
}

impl<'a, R: ProfilerCommandRecorder> std::ops::Deref for ManualOwningScope<'a, R> {
    type Target = R;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.recorder
    }
}

impl<'a, R: ProfilerCommandRecorder> std::ops::DerefMut for ManualOwningScope<'a, R> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.recorder
    }
}
