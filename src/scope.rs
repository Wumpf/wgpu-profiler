use crate::{GpuProfiler, ProfilerCommandRecorder};

pub struct Scope<'a, W: ProfilerCommandRecorder> {
    profiler: &'a mut GpuProfiler,
    recorder: &'a mut W,
}

pub struct OwningScope<'a, W: ProfilerCommandRecorder> {
    profiler: &'a mut GpuProfiler,
    recorder: W,
}

// Separate type since we can't destructure types that impl Drop :/
pub struct ManualOwningScope<'a, W: ProfilerCommandRecorder> {
    profiler: &'a mut GpuProfiler,
    recorder: W,
}

impl<'a, W: ProfilerCommandRecorder> Scope<'a, W> {
    pub fn start(profiler: &'a mut GpuProfiler, recorder: &'a mut W, device: &wgpu::Device, label: &str) -> Self {
        profiler.begin_scope(label, recorder, device);
        Self { profiler, recorder }
    }

    /// Starts a scope nested within this one
    pub fn scope(&mut self, device: &wgpu::Device, label: &str) -> Scope<'_, W> {
        Scope::start(self.profiler, self.recorder, device, label)
    }
}

impl<'a, W: ProfilerCommandRecorder> OwningScope<'a, W> {
    pub fn start(profiler: &'a mut GpuProfiler, mut recorder: W, device: &wgpu::Device, label: &str) -> Self {
        profiler.begin_scope(label, &mut recorder, device);
        Self { profiler, recorder }
    }

    /// Starts a scope nested within this one
    pub fn scope(&mut self, device: &wgpu::Device, label: &str) -> Scope<'_, W> {
        Scope::start(self.profiler, &mut self.recorder, device, label)
    }
}

impl<'a, W: ProfilerCommandRecorder> ManualOwningScope<'a, W> {
    pub fn start(profiler: &'a mut GpuProfiler, mut recorder: W, device: &wgpu::Device, label: &str) -> Self {
        profiler.begin_scope(label, &mut recorder, device);
        Self { profiler, recorder }
    }

    /// Starts a scope nested within this one
    pub fn scope(&mut self, device: &wgpu::Device, label: &str) -> Scope<'_, W> {
        Scope::start(self.profiler, &mut self.recorder, device, label)
    }

    /// Ends the scope allowing the extraction of owned the ProfilerCommandRecorder
    /// and the mutable reference to the GpuProfiler
    pub fn end_scope(mut self) -> (W, &'a mut GpuProfiler) {
        self.profiler.end_scope(&mut self.recorder);
        (self.recorder, self.profiler)
    }
}
impl<'a> Scope<'a, wgpu::CommandEncoder> {
    /// Start a render pass wrapped in an OwnedScope
    pub fn scoped_render_pass<'b>(
        &'b mut self,
        device: &wgpu::Device,
        label: &str,
        pass_descriptor: &wgpu::RenderPassDescriptor<'b, '_>,
    ) -> OwningScope<'b, wgpu::RenderPass<'b>> {
        let render_pass = self.recorder.begin_render_pass(pass_descriptor);
        OwningScope::start(self.profiler, render_pass, device, label)
    }

    pub fn scoped_compute_pass(
        &mut self,
        device: &wgpu::Device,
        label: &str,
        pass_descriptor: &wgpu::ComputePassDescriptor<'_>,
    ) -> OwningScope<wgpu::ComputePass> {
        let render_pass = self.recorder.begin_compute_pass(pass_descriptor);
        OwningScope::start(self.profiler, render_pass, device, label)
    }
}

impl<'a> OwningScope<'a, wgpu::CommandEncoder> {
    /// Start a render pass wrapped in an OwnedScope
    pub fn scoped_render_pass<'b>(
        &'b mut self,
        device: &wgpu::Device,
        label: &str,
        pass_descriptor: &wgpu::RenderPassDescriptor<'b, '_>,
    ) -> OwningScope<'b, wgpu::RenderPass<'b>> {
        let render_pass = self.recorder.begin_render_pass(pass_descriptor);
        OwningScope::start(self.profiler, render_pass, device, label)
    }

    pub fn scoped_compute_pass(
        &mut self,
        device: &wgpu::Device,
        label: &str,
        pass_descriptor: &wgpu::ComputePassDescriptor<'_>,
    ) -> OwningScope<wgpu::ComputePass> {
        let render_pass = self.recorder.begin_compute_pass(pass_descriptor);
        OwningScope::start(self.profiler, render_pass, device, label)
    }
}

impl<'a> ManualOwningScope<'a, wgpu::CommandEncoder> {
    /// Start a render pass wrapped in an OwnedScope
    pub fn scoped_render_pass<'b>(
        &'b mut self,
        device: &wgpu::Device,
        label: &str,
        pass_descriptor: &wgpu::RenderPassDescriptor<'b, '_>,
    ) -> OwningScope<'b, wgpu::RenderPass<'b>> {
        let render_pass = self.recorder.begin_render_pass(pass_descriptor);
        OwningScope::start(self.profiler, render_pass, device, label)
    }

    pub fn scoped_compute_pass(
        &mut self,
        device: &wgpu::Device,
        label: &str,
        pass_descriptor: &wgpu::ComputePassDescriptor<'_>,
    ) -> OwningScope<wgpu::ComputePass> {
        let render_pass = self.recorder.begin_compute_pass(pass_descriptor);
        OwningScope::start(self.profiler, render_pass, device, label)
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
        self.profiler.end_scope(self.recorder);
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
        self.profiler.end_scope(&mut self.recorder);
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
