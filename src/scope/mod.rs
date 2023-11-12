//! Scope types that wrap a `wgpu` encoder/pass and start a scope on creation. In most cases, they
//! then allow automatically ending the scope on drop.

mod manual_scope;
mod owning_scope;
mod scope;

pub use manual_scope::ManualOwningScope;
pub use owning_scope::OwningScope;
pub use scope::Scope;

use crate::ProfilerCommandRecorder;

/// The module is a workaround for `warning: private trait `ScopeAccessor` in public interface (error E0445)`
pub(crate) mod private {
    /// Unified access to scope parts, so we don't have to duplicate implementations for all types of scopes.
    pub trait ScopeAccessor<Recorder: crate::ProfilerCommandRecorder> {
        fn access(
            &mut self,
        ) -> (
            &crate::GpuProfiler,
            &mut Recorder,
            Option<&crate::GpuTimerScope>,
        );
    }
}

/// Methods implemented by all scope types.
pub trait ScopeExt<R>: private::ScopeAccessor<R>
where
    R: ProfilerCommandRecorder,
{
    /// Starts a new profiler scope nested within this one.
    #[must_use]
    #[track_caller]
    #[inline]
    fn scope(&mut self, label: impl Into<String>, device: &wgpu::Device) -> Scope<'_, R> {
        let (profiler, recorder, parent_scope) = self.access();
        Scope::start_nested(label, profiler, recorder, device, parent_scope)
    }
}

impl<R: ProfilerCommandRecorder, T: private::ScopeAccessor<R>> ScopeExt<R> for T {}

/// Methods implemented by all scope types that operate on command encoders.
pub trait EncoderScopeExt: private::ScopeAccessor<wgpu::CommandEncoder> {
    /// Start a render pass wrapped in a [`OwningScope`].
    ///
    /// Ignores passed `wgpu::RenderPassDescriptor::timestamp_writes` and replaces it with
    /// `timestamp_writes` managed by `GpuProfiler`.
    ///
    /// Note that in order to take measurements, this does not require the
    /// [`wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES`] feature, only [`wgpu::Features::TIMESTAMP_QUERY`].
    #[track_caller]
    fn scoped_render_pass<'a>(
        &'a mut self,
        label: impl Into<String>,
        device: &wgpu::Device,
        pass_descriptor: wgpu::RenderPassDescriptor<'a, '_>,
    ) -> OwningScope<'a, wgpu::RenderPass<'a>> {
        let (profiler, encoder, parent_scope) = self.access();
        let child_scope = profiler.begin_pass_scope(label, encoder, device, parent_scope);
        let render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            timestamp_writes: child_scope.render_pass_timestamp_writes(),
            ..pass_descriptor
        });

        OwningScope {
            profiler,
            recorder: render_pass,
            scope: Some(child_scope),
        }
    }

    /// Start a compute pass wrapped in a [`OwningScope`].
    ///
    /// Uses passed label both for profiler scope and compute pass label.
    /// `timestamp_writes` managed by `GpuProfiler`.
    ///
    /// Note that in order to take measurements, this does not require the
    /// [`wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES`] feature, only [`wgpu::Features::TIMESTAMP_QUERY`].
    #[track_caller]
    fn scoped_compute_pass<'a>(
        &'a mut self,
        label: impl Into<String>,
        device: &wgpu::Device,
    ) -> OwningScope<'a, wgpu::ComputePass<'a>> {
        let (profiler, encoder, parent_scope) = self.access();
        let child_scope = profiler.begin_pass_scope(label, encoder, device, parent_scope);

        let render_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some(&child_scope.label),
            timestamp_writes: child_scope.compute_pass_timestamp_writes(),
        });

        OwningScope {
            profiler,
            recorder: render_pass,
            scope: Some(child_scope),
        }
    }
}

impl<T: private::ScopeAccessor<wgpu::CommandEncoder>> EncoderScopeExt for T {}
