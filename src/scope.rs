//! Scope types that wrap a `wgpu` encoder/pass and start a scope on creation. In most cases, they
//! then allow automatically ending the scope on drop.

use crate::{GpuProfiler, ProfilerCommandRecorder};

/// Scope that takes a (mutable) reference to the encoder/pass.
///
/// Calls [`GpuProfiler::end_scope()`] on drop.
pub struct Scope<'a, W: ProfilerCommandRecorder> {
    profiler: &'a mut GpuProfiler,
    recorder: &'a mut W,
}

/// Scope that takes ownership of the encoder/pass.
///
/// Calls [`GpuProfiler::end_scope()`] on drop.
pub struct OwningScope<'a, W: ProfilerCommandRecorder> {
    profiler: &'a mut GpuProfiler,
    recorder: W,
}

/// Scope that takes ownership of the encoder/pass.
///
/// Does NOT call [`GpuProfiler::end_scope()`] on drop.
/// This construct is just for completeness in cases where working with scopes is preferred but one can't rely on the Drop call in the right place.
/// This is useful when the owned value needs to be recovered after the end of the scope.
/// In particular, to submit a [`wgpu::CommandEncoder`] to a queue, ownership of the encoder is necessary.
pub struct ManualOwningScope<'a, W: ProfilerCommandRecorder> {
    profiler: &'a mut GpuProfiler,
    recorder: W,
}

impl<'a, W: ProfilerCommandRecorder> Scope<'a, W> {
    /// Starts a new profiler scope. Scope is closed on drop.
    #[must_use]
    #[track_caller]
    pub fn start(label: &str, profiler: &'a mut GpuProfiler, recorder: &'a mut W, device: &wgpu::Device) -> Self {
        profiler.begin_scope(label, recorder, device);
        Self { profiler, recorder }
    }

    /// Starts a scope nested within this one.
    #[must_use]
    #[track_caller]
    pub fn scope(&mut self, label: &str, device: &wgpu::Device) -> Scope<'_, W> {
        Scope::start(label, self.profiler, self.recorder, device)
    }
}

impl<'a, W: ProfilerCommandRecorder> OwningScope<'a, W> {
    /// Starts a new profiler scope. Scope is closed on drop.
    #[must_use]
    #[track_caller]
    pub fn start(label: &str, profiler: &'a mut GpuProfiler, mut recorder: W, device: &wgpu::Device) -> Self {
        profiler.begin_scope(label, &mut recorder, device);
        Self { profiler, recorder }
    }

    /// Starts a scope nested within this one.
    #[must_use]
    #[track_caller]
    pub fn scope(&mut self, label: &str, device: &wgpu::Device) -> Scope<'_, W> {
        Scope::start(label, self.profiler, &mut self.recorder, device)
    }
}

impl<'a, W: ProfilerCommandRecorder> ManualOwningScope<'a, W> {
    /// Starts a new profiler scope. Scope is NOT closed on drop and needs to be closed manually with [`ManualOwningScope::end_scope`]
    #[must_use]
    #[track_caller]
    pub fn start(label: &str, profiler: &'a mut GpuProfiler, mut recorder: W, device: &wgpu::Device) -> Self {
        profiler.begin_scope(label, &mut recorder, device);
        Self { profiler, recorder }
    }

    /// Starts a scope nested within this one
    #[must_use]
    #[track_caller]
    pub fn scope(&mut self, label: &str, device: &wgpu::Device) -> Scope<'_, W> {
        Scope::start(label, self.profiler, &mut self.recorder, device)
    }

    /// Ends the scope allowing the extraction of the owned [`ProfilerCommandRecorder`]
    /// and the mutable reference to the [`GpuProfiler`].
    #[track_caller]
    pub fn end_scope(mut self) -> (W, &'a mut GpuProfiler) {
        // Can't fail since creation implies begin_scope.
        self.profiler.end_scope(&mut self.recorder).unwrap();
        (self.recorder, self.profiler)
    }
}

impl<'a> Scope<'a, wgpu::CommandEncoder> {
    /// Start a render pass wrapped in a [`OwningScope`].
    #[track_caller]
    pub fn scoped_render_pass<'b>(
        &'b mut self,
        label: &str,
        device: &wgpu::Device,
        pass_descriptor: &wgpu::RenderPassDescriptor<'b, '_>,
    ) -> OwningScope<'b, wgpu::RenderPass<'b>> {
        let render_pass = self.recorder.begin_render_pass(pass_descriptor);
        OwningScope::start(label, self.profiler, render_pass, device)
    }

    /// Start a compute pass wrapped in a [`OwningScope`].
    #[track_caller]
    pub fn scoped_compute_pass(
        &mut self,
        label: &str,
        device: &wgpu::Device,
        pass_descriptor: &wgpu::ComputePassDescriptor<'_>,
    ) -> OwningScope<wgpu::ComputePass> {
        let compute_pass = self.recorder.begin_compute_pass(pass_descriptor);
        OwningScope::start(label, self.profiler, compute_pass, device)
    }
}

impl<'a> OwningScope<'a, wgpu::CommandEncoder> {
    /// Start a render pass wrapped in an [`OwningScope`].
    #[track_caller]
    pub fn scoped_render_pass<'b>(
        &'b mut self,
        label: &str,
        device: &wgpu::Device,
        pass_descriptor: &wgpu::RenderPassDescriptor<'b, '_>,
    ) -> OwningScope<'b, wgpu::RenderPass<'b>> {
        let render_pass = self.recorder.begin_render_pass(pass_descriptor);
        OwningScope::start(label, self.profiler, render_pass, device)
    }

    /// Start a compute pass wrapped in a [`OwningScope`].
    #[track_caller]
    pub fn scoped_compute_pass(
        &mut self,
        label: &str,
        device: &wgpu::Device,
        pass_descriptor: &wgpu::ComputePassDescriptor<'_>,
    ) -> OwningScope<wgpu::ComputePass> {
        let compute_pass = self.recorder.begin_compute_pass(pass_descriptor);
        OwningScope::start(label, self.profiler, compute_pass, device)
    }
}

impl<'a> ManualOwningScope<'a, wgpu::CommandEncoder> {
    /// Start a render pass wrapped in an [`OwningScope`].
    #[track_caller]
    pub fn scoped_render_pass<'b>(
        &'b mut self,
        label: &str,
        device: &wgpu::Device,
        pass_descriptor: &wgpu::RenderPassDescriptor<'b, '_>,
    ) -> OwningScope<'b, wgpu::RenderPass<'b>> {
        let render_pass = self.recorder.begin_render_pass(pass_descriptor);
        OwningScope::start(label, self.profiler, render_pass, device)
    }

    /// Start a compute pass wrapped in an [`OwningScope`].
    #[track_caller]
    pub fn scoped_compute_pass(
        &mut self,
        label: &str,
        device: &wgpu::Device,
        pass_descriptor: &wgpu::ComputePassDescriptor<'_>,
    ) -> OwningScope<wgpu::ComputePass> {
        let compute_pass = self.recorder.begin_compute_pass(pass_descriptor);
        OwningScope::start(label, self.profiler, compute_pass, device)
    }
}

// Scope
impl<'a, W: ProfilerCommandRecorder> std::ops::Deref for Scope<'a, W> {
    type Target = W;

    fn deref(&self) -> &Self::Target {
        self.recorder
    }
}

impl<'a, W: ProfilerCommandRecorder> std::ops::DerefMut for Scope<'a, W> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.recorder
    }
}

impl<'a, W: ProfilerCommandRecorder> Drop for Scope<'a, W> {
    fn drop(&mut self) {
        // Creation implies begin_scope, so this can't fail.
        self.profiler.end_scope(self.recorder).unwrap();
    }
}

// OwningScope
impl<'a, W: ProfilerCommandRecorder> std::ops::Deref for OwningScope<'a, W> {
    type Target = W;

    fn deref(&self) -> &Self::Target {
        &self.recorder
    }
}

impl<'a, W: ProfilerCommandRecorder> std::ops::DerefMut for OwningScope<'a, W> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.recorder
    }
}

impl<'a, W: ProfilerCommandRecorder> Drop for OwningScope<'a, W> {
    fn drop(&mut self) {
        // Creation implies begin_scope, so this can't fail.
        self.profiler.end_scope(&mut self.recorder).unwrap();
    }
}

// ManualOwningScope
impl<'a, W: ProfilerCommandRecorder> std::ops::Deref for ManualOwningScope<'a, W> {
    type Target = W;

    fn deref(&self) -> &Self::Target {
        &self.recorder
    }
}

impl<'a, W: ProfilerCommandRecorder> std::ops::DerefMut for ManualOwningScope<'a, W> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.recorder
    }
}
