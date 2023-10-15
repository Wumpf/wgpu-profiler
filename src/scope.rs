//! Scope types that wrap a `wgpu` encoder/pass and start a scope on creation. In most cases, they
//! then allow automatically ending the scope on drop.

use crate::{GpuProfiler, OpenTimerScope, ProfilerCommandRecorder};

/// Scope that takes a (mutable) reference to the encoder/pass.
///
/// Calls [`GpuProfiler::end_scope()`] on drop.
pub struct Scope<'a, W: ProfilerCommandRecorder> {
    profiler: &'a mut GpuProfiler,
    recorder: &'a mut W,
    open_scope: OpenTimerScope<'a>,
}

/// Scope that takes ownership of the encoder/pass.
///
/// Calls [`GpuProfiler::end_scope()`] on drop.
pub struct OwningScope<'a, W: ProfilerCommandRecorder> {
    profiler: &'a mut GpuProfiler,
    recorder: W,
    open_scope: OpenTimerScope<'a>,
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
    pub open_scope: OpenTimerScope<'a>,
}

impl<'a, W: ProfilerCommandRecorder> Scope<'a, W> {
    /// Starts a new profiler scope. Scope is closed on drop.
    #[must_use]
    #[track_caller]
    pub fn start(
        label: &str,
        profiler: &'a mut GpuProfiler,
        recorder: &'a mut W,
        device: &wgpu::Device,
    ) -> Self {
        let open_scope = profiler.begin_scope(label, recorder, device, None);
        Self {
            profiler,
            recorder,
            open_scope,
        }
    }

    /// Starts a new profiler scope nested in a previous one. Scope is closed on drop.
    #[must_use]
    #[track_caller]
    pub fn start_nested(
        label: &str,
        profiler: &'a mut GpuProfiler,
        recorder: &'a mut W,
        device: &wgpu::Device,
        parent_scope: &'a mut OpenTimerScope<'a>,
    ) -> Self {
        let open_scope = profiler.begin_scope(label, recorder, device, Some(parent_scope));
        Self {
            profiler,
            recorder,
            open_scope,
        }
    }

    /// Starts a scope nested within this one.
    #[must_use]
    #[track_caller]
    pub fn scope<'b: 'a>(&'b mut self, label: &str, device: &wgpu::Device) -> Scope<'_, W> {
        let open_scope =
            self.profiler
                .begin_scope(label, self.recorder, device, Some(&mut self.open_scope));
        Self {
            profiler: self.profiler,
            recorder: self.recorder,
            open_scope,
        }
    }
}

impl<'a, 'b: 'a, W: ProfilerCommandRecorder> OwningScope<'b, W> {
    /// Starts a new profiler scope. Scope is closed on drop.
    #[must_use]
    #[track_caller]
    pub fn start(
        label: &str,
        profiler: &'b mut GpuProfiler,
        mut recorder: W,
        device: &wgpu::Device,
    ) -> Self {
        let open_scope = profiler.begin_scope(label, &mut recorder, device, None);
        Self {
            profiler,
            recorder,
            open_scope,
        }
    }

    /// Starts a new profiler scope nested in a previous one. Scope is closed on drop.
    #[must_use]
    #[track_caller]
    pub fn start_nested(
        label: &str,
        profiler: &'b mut GpuProfiler,
        mut recorder: W,
        device: &wgpu::Device,
        parent_scope: &'b mut OpenTimerScope<'b>,
    ) -> Self {
        let open_scope = profiler.begin_scope(label, &mut recorder, device, Some(parent_scope));
        Self {
            profiler,
            recorder,
            open_scope,
        }
    }

    /// Starts a scope nested within this one.
    #[must_use]
    #[track_caller]
    pub fn scope(&'b mut self, label: &str, device: &wgpu::Device) -> Scope<'a, W> {
        let open_scope = self.profiler.begin_scope(
            label,
            &mut self.recorder,
            device,
            Some(&mut self.open_scope),
        );
        Scope {
            profiler: self.profiler,
            recorder: &mut self.recorder,
            open_scope,
        }
    }
}

impl<'a, W: ProfilerCommandRecorder> ManualOwningScope<'a, W> {
    /// Starts a new profiler scope. Scope is NOT closed on drop and needs to be closed manually with [`ManualOwningScope::end_scope`]
    #[must_use]
    #[track_caller]
    pub fn start(
        label: &str,
        profiler: &'a mut GpuProfiler,
        mut recorder: W,
        device: &wgpu::Device,
    ) -> Self {
        let open_scope = profiler.begin_scope(label, &mut recorder, device, None);
        Self {
            profiler,
            recorder,
            open_scope,
        }
    }

    /// Starts a new profiler scope nested in a previous one.
    /// Scope is NOT closed on drop and needs to be closed manually with [`ManualOwningScope::end_scope`]
    #[must_use]
    #[track_caller]
    pub fn start_nested(
        label: &str,
        profiler: &'a mut GpuProfiler,
        mut recorder: W,
        device: &wgpu::Device,
        parent_scope: &'a mut OpenTimerScope<'a>,
    ) -> Self {
        let open_scope = profiler.begin_scope(label, &mut recorder, device, Some(parent_scope));
        Self {
            profiler,
            recorder,
            open_scope,
        }
    }

    /// Starts a scope nested within this one
    #[must_use]
    #[track_caller]
    pub fn scope(&'a mut self, label: &str, device: &wgpu::Device) -> Scope<'_, W> {
        let open_scope = self.profiler.begin_scope(
            label,
            &mut self.recorder,
            device,
            Some(&mut self.open_scope),
        );
        Scope {
            profiler: self.profiler,
            recorder: &mut self.recorder,
            open_scope,
        }
    }

    /// Ends the scope allowing the extraction of the owned [`ProfilerCommandRecorder`]
    /// and the mutable reference to the [`GpuProfiler`].
    #[track_caller]
    pub fn end_scope(self) -> (W, &'a mut GpuProfiler) {
        let ManualOwningScope {
            profiler,
            mut recorder,
            open_scope,
        } = self;

        profiler.end_scope(&mut recorder, open_scope);
        (recorder, profiler)
    }
}

impl<'a> Scope<'a, wgpu::CommandEncoder> {
    /// Start a render pass wrapped in a [`OwningScope`].
    #[track_caller]
    pub fn scoped_render_pass<'b: 'a>(
        &'b mut self,
        label: &str,
        device: &wgpu::Device,
        pass_descriptor: &wgpu::RenderPassDescriptor<'b, '_>,
    ) -> OwningScope<'_, wgpu::RenderPass<'b>> {
        let render_pass = self.recorder.begin_render_pass(pass_descriptor);
        OwningScope::start_nested(
            label,
            self.profiler,
            render_pass,
            device,
            &mut self.open_scope,
        )
    }

    /// Start a compute pass wrapped in a [`OwningScope`].
    #[track_caller]
    pub fn scoped_compute_pass<'b: 'a>(
        &'b mut self,
        label: &str,
        device: &wgpu::Device,
        pass_descriptor: &wgpu::ComputePassDescriptor<'_>,
    ) -> OwningScope<'_, wgpu::ComputePass> {
        let compute_pass = self.recorder.begin_compute_pass(pass_descriptor);
        OwningScope::start_nested(
            label,
            self.profiler,
            compute_pass,
            device,
            &mut self.open_scope,
        )
    }
}

// impl<'a> OwningScope<'a, wgpu::CommandEncoder> {
//     /// Start a render pass wrapped in an [`OwningScope`].
//     #[track_caller]
//     pub fn scoped_render_pass<'b: 'a>(
//         &'b mut self,
//         label: &str,
//         device: &wgpu::Device,
//         pass_descriptor: &wgpu::RenderPassDescriptor<'b, '_>,
//     ) -> OwningScope<'b, wgpu::RenderPass<'b>> {
//         let render_pass = self.recorder.begin_render_pass(pass_descriptor);
//         OwningScope::start(label, self.profiler, render_pass, device)
//     }

//     /// Start a compute pass wrapped in a [`OwningScope`].
//     #[track_caller]
//     pub fn scoped_compute_pass<'b: 'a>(
//         &mut self,
//         label: &str,
//         device: &wgpu::Device,
//         pass_descriptor: &wgpu::ComputePassDescriptor<'_>,
//     ) -> OwningScope<'b, wgpu::ComputePass> {
//         let compute_pass = self.recorder.begin_compute_pass(pass_descriptor);
//         OwningScope::start(label, self.profiler, compute_pass, device)
//     }
// }

// impl<'a> ManualOwningScope<'a, wgpu::CommandEncoder> {
//     /// Start a render pass wrapped in an [`OwningScope`].
//     #[track_caller]
//     pub fn scoped_render_pass<'b: 'a>(
//         &'b mut self,
//         label: &str,
//         device: &wgpu::Device,
//         pass_descriptor: &wgpu::RenderPassDescriptor<'b, '_>,
//     ) -> OwningScope<'b, wgpu::RenderPass<'b>> {
//         let render_pass = self.recorder.begin_render_pass(pass_descriptor);
//         OwningScope::start(label, self.profiler, render_pass, device)
//     }

//     /// Start a compute pass wrapped in an [`OwningScope`].
//     #[track_caller]
//     pub fn scoped_compute_pass<'b: 'a>(
//         &mut self,
//         label: &str,
//         device: &wgpu::Device,
//         pass_descriptor: &wgpu::ComputePassDescriptor<'_>,
//     ) -> OwningScope<'b, wgpu::ComputePass> {
//         let compute_pass = self.recorder.begin_compute_pass(pass_descriptor);
//         OwningScope::start(label, self.profiler, compute_pass, device)
//     }
// }

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
        self.profiler
            .end_scope(self.recorder, self.open_scope.take());
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
        self.profiler
            .end_scope(&mut self.recorder, self.open_scope.take());
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
