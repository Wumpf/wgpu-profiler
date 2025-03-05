//! Scope types that wrap a `wgpu` encoder/pass and start a scope on creation. In most cases, they
//! then allow automatically ending the scope on drop.

use crate::{GpuProfiler, GpuProfilerQuery, ProfilerCommandRecorder};

/// Scope that takes a (mutable) reference to the encoder/pass.
///
/// Calls [`GpuProfiler::end_query()`] on drop.
pub struct Scope<'a, Recorder: ProfilerCommandRecorder> {
    pub profiler: &'a GpuProfiler,
    pub recorder: &'a mut Recorder,
    pub scope: Option<GpuProfilerQuery>,
}

impl<R: ProfilerCommandRecorder> Drop for Scope<'_, R> {
    #[inline]
    fn drop(&mut self) {
        if let Some(scope) = self.scope.take() {
            self.profiler.end_query(self.recorder, scope);
        }
    }
}

/// Scope that takes ownership of the encoder/pass.
///
/// Calls [`GpuProfiler::end_query()`] on drop.
pub struct OwningScope<'a, Recorder: ProfilerCommandRecorder> {
    pub profiler: &'a GpuProfiler,
    pub recorder: Recorder,
    pub scope: Option<GpuProfilerQuery>,
}

impl<R: ProfilerCommandRecorder> Drop for OwningScope<'_, R> {
    #[inline]
    fn drop(&mut self) {
        if let Some(scope) = self.scope.take() {
            self.profiler.end_query(&mut self.recorder, scope);
        }
    }
}

/// Scope that takes ownership of the encoder/pass.
///
/// Does NOT call [`GpuProfiler::end_query()`] on drop.
/// This construct is just for completeness in cases where working with scopes is preferred but one can't rely on the Drop call in the right place.
/// This is useful when the owned value needs to be recovered after the end of the scope.
/// In particular, to submit a [`wgpu::CommandEncoder`] to a queue, ownership of the encoder is necessary.
pub struct ManualOwningScope<'a, Recorder: ProfilerCommandRecorder> {
    pub profiler: &'a GpuProfiler,
    pub recorder: Recorder,
    pub scope: Option<GpuProfilerQuery>,
}

impl<R: ProfilerCommandRecorder> ManualOwningScope<'_, R> {
    /// Ends the scope allowing the extraction of the owned [`ProfilerCommandRecorder`].
    #[track_caller]
    #[inline]
    pub fn end_query(mut self) -> R {
        // Can't fail since creation implies begin_query.
        self.profiler
            .end_query(&mut self.recorder, self.scope.take().unwrap());
        self.recorder
    }
}

/// Most implementation code of the different scope types is exactly the same.
///
/// This macro allows to avoid code duplication.
/// Another way of achieving this are extension traits, but this would mean that a user has to
/// import the extension trait to use all methods of the scope types which I found a bit annoying.
macro_rules! impl_scope_ext {
    ($scope:ident, $recorder_type:ty) => {
        impl<'a, R: ProfilerCommandRecorder> $scope<'a, R> {
            /// Starts a new profiler scope nested within this one.
            #[must_use]
            #[track_caller]
            #[inline]
            pub fn scope(&mut self, label: impl Into<String>) -> Scope<'_, R> {
                let recorder: &mut R = &mut self.recorder;
                let scope = self
                    .profiler
                    .begin_query(label, recorder)
                    .with_parent(self.scope.as_ref());
                Scope {
                    profiler: self.profiler,
                    recorder,
                    scope: Some(scope),
                }
            }
        }

        impl<'a> $scope<'a, wgpu::CommandEncoder> {
            /// Start a render pass wrapped in a [`OwningScope`].
            ///
            /// Ignores passed `wgpu::RenderPassDescriptor::timestamp_writes` and replaces it with
            /// `timestamp_writes` managed by `GpuProfiler` if profiling is enabled.
            ///
            /// This also sets the `wgpu::RenderPassDescriptor::label` if it's `None` (default).
            ///
            /// Note that in order to take measurements, this requires the [`wgpu::Features::TIMESTAMP_QUERY`] feature.
            /// [`wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS`] & [`wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES`] are not required.
            #[track_caller]
            pub fn scoped_render_pass(
                &mut self,
                label: impl Into<String>,
                pass_descriptor: wgpu::RenderPassDescriptor<'_>,
            ) -> OwningScope<'_, wgpu::RenderPass<'_>> {
                let child_scope = self
                    .profiler
                    .begin_pass_query(label, &mut self.recorder)
                    .with_parent(self.scope.as_ref());
                let render_pass = self
                    .recorder
                    .begin_render_pass(&wgpu::RenderPassDescriptor {
                        timestamp_writes: child_scope.render_pass_timestamp_writes(),
                        label: pass_descriptor.label.or(Some(&child_scope.label)),
                        ..pass_descriptor
                    });

                OwningScope {
                    profiler: self.profiler,
                    recorder: render_pass,
                    scope: Some(child_scope),
                }
            }

            /// Start a compute pass wrapped in a [`OwningScope`].
            ///
            /// Uses passed label both for profiler scope and compute pass label.
            /// `timestamp_writes` managed by `GpuProfiler` if profiling is enabled.
            ///
            /// Note that in order to take measurements, this requires the [`wgpu::Features::TIMESTAMP_QUERY`] feature.
            /// [`wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS`] & [`wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES`] are not required.
            #[track_caller]
            pub fn scoped_compute_pass(
                &mut self,
                label: impl Into<String>,
            ) -> OwningScope<'_, wgpu::ComputePass<'_>> {
                let child_scope = self
                    .profiler
                    .begin_pass_query(label, &mut self.recorder)
                    .with_parent(self.scope.as_ref());

                let render_pass = self
                    .recorder
                    .begin_compute_pass(&wgpu::ComputePassDescriptor {
                        label: Some(&child_scope.label),
                        timestamp_writes: child_scope.compute_pass_timestamp_writes(),
                    });

                OwningScope {
                    profiler: self.profiler,
                    recorder: render_pass,
                    scope: Some(child_scope),
                }
            }
        }

        impl<'a, R: ProfilerCommandRecorder> std::ops::Deref for $scope<'a, R> {
            type Target = R;

            #[inline]
            fn deref(&self) -> &Self::Target {
                &self.recorder
            }
        }

        impl<'a, R: ProfilerCommandRecorder> std::ops::DerefMut for $scope<'a, R> {
            #[inline]
            fn deref_mut(&mut self) -> &mut Self::Target {
                &mut self.recorder
            }
        }
    };
}

impl_scope_ext!(Scope, &'a mut R);
impl_scope_ext!(OwningScope, R);
impl_scope_ext!(ManualOwningScope, R);
