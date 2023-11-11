//! Scope types that wrap a `wgpu` encoder/pass and start a scope on creation. In most cases, they
//! then allow automatically ending the scope on drop.

use crate::{GpuProfiler, GpuTimerScope, ProfilerCommandRecorder};

/// Scope that takes a (mutable) reference to the encoder/pass.
///
/// Calls [`GpuProfiler::end_scope()`] on drop.
pub struct Scope<'a, Recorder: ProfilerCommandRecorder> {
    pub profiler: &'a GpuProfiler,
    pub recorder: &'a mut Recorder,
    pub scope: Option<GpuTimerScope>,
}

/// Scope that takes ownership of the encoder/pass.
///
/// Calls [`GpuProfiler::end_scope()`] on drop.
pub struct OwningScope<'a, Recorder: ProfilerCommandRecorder> {
    pub profiler: &'a GpuProfiler,
    pub recorder: Recorder,
    pub scope: Option<GpuTimerScope>,
}

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

    /// Starts a new profiler scope nested within this one.
    #[must_use]
    #[track_caller]
    #[inline]
    pub fn scope(&mut self, label: impl Into<String>, device: &wgpu::Device) -> Scope<'_, W> {
        Scope::start_nested(
            label,
            self.profiler,
            self.recorder,
            device,
            self.scope.as_ref(),
        )
    }
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

    /// Starts a new profiler scope nested within this one.
    #[must_use]
    #[track_caller]
    #[inline]
    pub fn scope(&mut self, label: impl Into<String>, device: &wgpu::Device) -> Scope<'_, W> {
        Scope::start_nested(
            label,
            self.profiler,
            &mut self.recorder,
            device,
            self.scope.as_ref(),
        )
    }
}

impl<'a, W: ProfilerCommandRecorder> ManualOwningScope<'a, W> {
    /// Starts a new profiler scope.
    ///
    /// Scope is NOT closed on drop and needs to be closed manually with [`ManualOwningScope::end_scope`]
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

    /// Starts a new profiler scope nested in another one.
    ///
    /// Scope is NOT closed on drop and needs to be closed manually with [`ManualOwningScope::end_scope`]
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

    /// Starts a new profiler scope nested within this one.
    ///
    /// Scope is NOT closed on drop and needs to be closed manually with [`ManualOwningScope::end_scope`]
    #[must_use]
    #[track_caller]
    #[inline]
    pub fn scope(&mut self, label: impl Into<String>, device: &wgpu::Device) -> Scope<'_, W> {
        Scope::start(label, self.profiler, &mut self.recorder, device)
    }

    /// Ends the scope allowing the extraction of the owned [`ProfilerCommandRecorder`].
    #[track_caller]
    #[inline]
    pub fn end_scope(mut self) -> W {
        // Can't fail since creation implies begin_scope.
        self.profiler
            .end_scope(&mut self.recorder, self.scope.take().unwrap());
        self.recorder
    }
}

impl<'a> Scope<'a, wgpu::CommandEncoder> {
    /// Start a render pass wrapped in a [`OwningScope`].
    ///
    /// TODO(#51): Use `RenderPassDescriptor::timestamp_writes`
    #[track_caller]
    #[inline]
    pub fn scoped_render_pass<'b>(
        &'b mut self,
        label: impl Into<String>,
        device: &wgpu::Device,
        pass_descriptor: &wgpu::RenderPassDescriptor<'b, '_>,
    ) -> OwningScope<'b, wgpu::RenderPass<'b>> {
        let render_pass = self.recorder.begin_render_pass(pass_descriptor);
        OwningScope::start_nested(
            label,
            self.profiler,
            render_pass,
            device,
            self.scope.as_ref(),
        )
    }

    /// Start a compute pass wrapped in a [`OwningScope`].
    ///
    /// TODO(#51): Use `ComputePassDescriptor::timestamp_writes`
    #[track_caller]
    #[inline]
    pub fn scoped_compute_pass(
        &mut self,
        label: impl Into<String>,
        device: &wgpu::Device,
        pass_descriptor: &wgpu::ComputePassDescriptor<'_>,
    ) -> OwningScope<wgpu::ComputePass> {
        let compute_pass = self.recorder.begin_compute_pass(pass_descriptor);
        OwningScope::start_nested(
            label,
            self.profiler,
            compute_pass,
            device,
            self.scope.as_ref(),
        )
    }
}

impl<'a> OwningScope<'a, wgpu::CommandEncoder> {
    /// Start a render pass wrapped in an [`OwningScope`].
    ///
    /// TODO(#51): Use `RenderPassDescriptor::timestamp_writes`
    #[track_caller]
    #[inline]
    pub fn scoped_render_pass<'b>(
        &'b mut self,
        label: impl Into<String>,
        device: &wgpu::Device,
        pass_descriptor: &wgpu::RenderPassDescriptor<'b, '_>,
    ) -> OwningScope<'b, wgpu::RenderPass<'b>> {
        let render_pass = self.recorder.begin_render_pass(pass_descriptor);
        OwningScope::start_nested(
            label,
            self.profiler,
            render_pass,
            device,
            self.scope.as_ref(),
        )
    }

    /// Start a compute pass wrapped in a [`OwningScope`].
    ///
    /// TODO(#51): Use `ComputePassDescriptor::timestamp_writes`
    #[track_caller]
    #[inline]
    pub fn scoped_compute_pass(
        &mut self,
        label: impl Into<String>,
        device: &wgpu::Device,
        pass_descriptor: &wgpu::ComputePassDescriptor<'_>,
    ) -> OwningScope<wgpu::ComputePass> {
        let compute_pass = self.recorder.begin_compute_pass(pass_descriptor);
        OwningScope::start_nested(
            label,
            self.profiler,
            compute_pass,
            device,
            self.scope.as_ref(),
        )
    }
}

impl<'a> ManualOwningScope<'a, wgpu::CommandEncoder> {
    /// Start a render pass wrapped in an [`OwningScope`].
    ///
    /// TODO(#51): Use `RenderPassDescriptor::timestamp_writes`
    #[track_caller]
    #[inline]
    pub fn scoped_render_pass<'b>(
        &'b mut self,
        label: impl Into<String>,
        device: &wgpu::Device,
        pass_descriptor: &wgpu::RenderPassDescriptor<'b, '_>,
    ) -> OwningScope<'b, wgpu::RenderPass<'b>> {
        let render_pass = self.recorder.begin_render_pass(pass_descriptor);
        OwningScope::start_nested(
            label,
            self.profiler,
            render_pass,
            device,
            self.scope.as_ref(),
        )
    }

    /// Start a compute pass wrapped in an [`OwningScope`].
    ///
    /// TODO(#51): Use `ComputePassDescriptor::timestamp_writes`
    #[track_caller]
    #[inline]
    pub fn scoped_compute_pass(
        &mut self,
        label: impl Into<String>,
        device: &wgpu::Device,
        pass_descriptor: &wgpu::ComputePassDescriptor<'_>,
    ) -> OwningScope<wgpu::ComputePass> {
        let compute_pass = self.recorder.begin_compute_pass(pass_descriptor);
        OwningScope::start_nested(
            label,
            self.profiler,
            compute_pass,
            device,
            self.scope.as_ref(),
        )
    }
}

// Scope
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

impl<'a, W: ProfilerCommandRecorder> Drop for Scope<'a, W> {
    #[inline]
    fn drop(&mut self) {
        // Creation implies begin_scope, so this can't fail.
        self.profiler
            .end_scope(self.recorder, self.scope.take().unwrap());
    }
}

// OwningScope
impl<'a, W: ProfilerCommandRecorder> std::ops::Deref for OwningScope<'a, W> {
    type Target = W;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.recorder
    }
}

impl<'a, W: ProfilerCommandRecorder> std::ops::DerefMut for OwningScope<'a, W> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.recorder
    }
}

impl<'a, W: ProfilerCommandRecorder> Drop for OwningScope<'a, W> {
    #[inline]
    fn drop(&mut self) {
        // Creation implies begin_scope, so this can't fail.
        self.profiler
            .end_scope(&mut self.recorder, self.scope.take().unwrap());
    }
}

// ManualOwningScope
impl<'a, W: ProfilerCommandRecorder> std::ops::Deref for ManualOwningScope<'a, W> {
    type Target = W;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.recorder
    }
}

impl<'a, W: ProfilerCommandRecorder> std::ops::DerefMut for ManualOwningScope<'a, W> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.recorder
    }
}
