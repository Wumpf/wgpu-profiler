/*!

Easy to use profiler scopes for [wgpu](https://github.com/gfx-rs/wgpu) using timer queries.

`wgpu_profiler` manages all the necessary [`wgpu::QuerySet`] and [`wgpu::Buffer`] behind the scenes
and allows you to create to create timer scopes with minimal overhead!

# How to use

```
use wgpu_profiler::*;

# async fn wgpu_init() -> (wgpu::Instance, wgpu::Adapter, wgpu::Device, wgpu::Queue) {
    # let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
    # let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions::default()).await.unwrap();
    # let (device, queue) = adapter
    #     .request_device(
    #         &wgpu::DeviceDescriptor {
    #             features: wgpu::Features::TIMESTAMP_QUERY,
    #             ..Default::default()
    #         },
    #         None,
    #     )
    #     .await
    #     .unwrap();
    # (instance, adapter, device, queue)
# }
# let (instance, adapter, device, queue) = futures_lite::future::block_on(wgpu_init());
# let cs_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
#     label: None,
#     source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(include_str!("../examples/compute_shader.wgsl"))),
# });
# let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
#        label: None,
#        layout: None,
#        module: &cs_module,
#        entry_point: "main",
#    });
// ...

let mut profiler = GpuProfiler::new(GpuProfilerSettings::default()).unwrap();

// ...

# let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
{
    // You can now open profiling scopes on any encoder or pass:
    let mut scope = profiler.scope("name of your scope", &mut encoder, &device);

    // Scopes can be nested arbitrarily!
    let mut nested_scope = scope.scope("nested!", &device);

    // Scopes on encoders can be used to easily create profiled passes!
    let mut compute_pass = nested_scope.scoped_compute_pass("profiled compute", &device);


    // Scopes expose the underlying encoder or pass they wrap:
    compute_pass.set_pipeline(&pipeline);
    // ...

    // Scopes created this way are automatically closed when dropped.
}

// Wgpu-profiler needs to insert buffer copy commands.
profiler.resolve_queries(&mut encoder);
# drop(encoder);

// ...

// And finally, to end a profiling frame, call `end_frame`.
// This does a few checks and will let you know if something is off!
profiler.end_frame().unwrap();

// Retrieving the oldest available frame and writing it out to a chrome trace file.
if let Some(profiling_data) = profiler.process_finished_frame(queue.get_timestamp_period()) {
    # let button_pressed = false;
    // You usually want to write to disk only under some condition, e.g. press of a key.
    if button_pressed {
        wgpu_profiler::chrometrace::write_chrometrace(
            std::path::Path::new("mytrace.json"), &profiling_data);
    }
}
```
Check also the [Example](https://github.com/Wumpf/wgpu-profiler/blob/main/examples/demo.rs) where everything can be seen in action.

# Internals

For every frame that hasn't completely finished processing yet
(i.e. hasn't returned results via [`GpuProfiler::process_finished_frame`])
we keep a `PendingFrame` around.

Whenever a profiling scope is opened, we allocate two queries.
This is done by either using the most recent `QueryPool` or creating a new one if there's no non-exhausted one ready.
Ideally, we only ever need a single `QueryPool` per frame! In order to converge to this,
we allocate new query pools with the size of all previous query pools in a given frame, effectively doubling the size.
On [`GpuProfiler::end_frame`], we memorize the total size of all `QueryPool`s in the current frame and make this the new minimum pool size.

`QueryPool` from finished frames are re-used, unless they are deemed too small.
*/

pub mod chrometrace;
mod errors;
mod scope;
#[cfg(feature = "tracy")]
mod tracy;

pub use errors::{CreationError, EndFrameError, SettingsError};
pub use scope::{ManualOwningScope, OwningScope, Scope};

// ---------------

use std::{
    collections::HashMap,
    ops::Range,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
    thread::ThreadId,
};

use parking_lot::{Mutex, RwLock};

/// The result of a gpu timer scope.
#[derive(Debug, Clone)]
pub struct GpuTimerScopeResult {
    /// Label that was specified when opening the scope.
    pub label: String,

    /// The process id of the process that opened this scope.
    pub pid: u32,

    /// The thread id of the thread that opened this scope.
    pub tid: ThreadId,

    /// Time range of this scope in seconds.
    ///
    /// Meaning of absolute value is not defined.
    pub time: Range<f64>,

    /// Scopes that were opened while this scope was open.
    pub nested_scopes: Vec<GpuTimerScopeResult>,
}

/// An in-flight GPU timer scope.
///
/// *Must* be closed by calling [`GpuProfiler::end_scope`].
/// Will cause debug assertion if dropped without being closed.
///
/// Emitted by [`GpuProfiler::begin_scope`] and consumed by [`GpuProfiler::end_scope`].
pub struct GpuTimerScope {
    /// The label assigned to this scope.
    /// Will be moved into [`GpuTimerScopeResult::label`] once the scope is fully processed.
    pub label: String,

    /// The process id of the process that opened this scope.
    pub pid: u32,

    /// The thread id of the thread that opened this scope.
    pub tid: ThreadId,

    /// The actual query on a query pool if any (none if disabled for this type of scope).
    query: Option<ReservedQueryPair>,

    /// Handle which identifies this scope, used for building the tree of scopes.
    handle: GpuTimerScopeTreeHandle,

    /// Which scope this scope is a child of.
    parent_handle: GpuTimerScopeTreeHandle,

    /// Whether a debug group was opened for this scope.
    has_debug_group: bool,

    #[cfg(feature = "tracy")]
    tracy_scope: Option<tracy_client::GpuSpan>,
}

impl GpuTimerScope {
    /// Use the reserved query for render pass timestamp writes if any.
    ///
    /// Use this only for a single render/compute pass, otherwise results will be overwritten.
    pub fn render_pass_timestamp_writes(&self) -> Option<wgpu::RenderPassTimestampWrites> {
        self.query
            .as_ref()
            .map(|query| wgpu::RenderPassTimestampWrites {
                query_set: &query.pool.query_set,
                beginning_of_pass_write_index: Some(query.start_query_idx),
                end_of_pass_write_index: Some(query.start_query_idx + 1),
            })
    }

    /// Use the reserved query for compute pass timestamp writes if any.
    ///
    /// Use this only for a single render/compute pass, otherwise results will be overwritten.
    pub fn compute_pass_timestamp_writes(&self) -> Option<wgpu::ComputePassTimestampWrites> {
        self.query
            .as_ref()
            .map(|query| wgpu::ComputePassTimestampWrites {
                query_set: &query.pool.query_set,
                beginning_of_pass_write_index: Some(query.start_query_idx),
                end_of_pass_write_index: Some(query.start_query_idx + 1),
            })
    }

    /// Makes this scope a child of the passed scope.
    #[inline]
    pub fn with_parent(self, parent: Option<&GpuTimerScope>) -> Self {
        Self {
            parent_handle: parent.map_or(ROOT_SCOPE_HANDLE, |p| p.handle),
            ..self
        }
    }
}

/// Settings passed on initialization of [`GpuProfiler`].
#[derive(Debug, Clone)]
pub struct GpuProfilerSettings {
    /// Enables/disables the profiler.
    ///
    /// If false, the profiler will not emit any timer queries, making most operations on [`GpuProfiler`] no-ops.
    ///
    /// Since all resource creation is done lazily, this provides an effective way of disabling the profiler at runtime
    /// without the need of special build configurations or code to handle enabled/disabled profiling.
    pub enable_timer_scopes: bool,

    /// Enables/disables debug markers for all scopes on the respective encoder or pass.
    ///
    /// This is useful for debugging with tools like RenderDoc.
    /// Debug markers will be emitted even if the device does not support timer queries or disables them via
    /// [`GpuProfilerSettings::enable_timer_scopes`].
    pub enable_debug_groups: bool,

    /// The profiler queues up to `max_num_pending_frames` "profiler-frames" at a time.
    ///
    /// A profiler-frame is regarded as in-flight until its queries have been successfully
    /// resolved using [`GpuProfiler::process_finished_frame`].
    /// How long this takes to happen, depends on how fast buffer mappings return successfully
    /// which in turn primarily depends on how fast the device is able to finish work queued to the [`wgpu::Queue`].
    ///
    /// If this threshold is exceeded, [`GpuProfiler::end_frame`] will silently drop frames.
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
            enable_timer_scopes: true,
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

/// Profiler instance.
///
/// You can have an arbitrary number of independent profiler instances per application/adapter.
/// Manages all the necessary [`wgpu::QuerySet`] and [`wgpu::Buffer`] behind the scenes.
///
/// Any scope opening method may allocate a new [`wgpu::QuerySet`] and [`wgpu::Buffer`] internally if necessary.
///
/// After the first call that passes [`wgpu::Device`], the same device must be used with all subsequent
/// calls to [`GpuProfiler`] and all passed references to wgpu objects must originate from that device.
pub struct GpuProfiler {
    unused_pools: Vec<QueryPool>,

    active_frame: ActiveFrame,
    pending_frames: Vec<PendingFrame>,

    num_open_scopes: AtomicU32,
    next_scope_handle: AtomicU32,

    size_for_new_query_pools: u32,

    settings: GpuProfilerSettings,

    #[cfg(feature = "tracy")]
    tracy_context: Option<tracy_client::GpuContext>,
}

// Public interface
impl GpuProfiler {
    /// Combination of all timer query features [`GpuProfiler`] can leverage.
    pub const ALL_WGPU_TIMER_FEATURES: wgpu::Features =
        wgpu::Features::TIMESTAMP_QUERY.union(wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES);

    /// Combination of all timer query features [`GpuProfiler`] can leverage.
    #[deprecated(since = "0.9.0", note = "Use ALL_WGPU_TIMER_FEATURES instead")]
    pub const REQUIRED_WGPU_FEATURES: wgpu::Features = GpuProfiler::ALL_WGPU_TIMER_FEATURES;

    /// Creates a new Profiler object.
    ///
    /// There is nothing preventing the use of several independent profiler objects.
    pub fn new(settings: GpuProfilerSettings) -> Result<Self, CreationError> {
        settings.validate()?;

        let (closed_scope_sender, closed_scope_receiver) = std::sync::mpsc::channel();

        Ok(GpuProfiler {
            unused_pools: Vec::new(),

            pending_frames: Vec::with_capacity(settings.max_num_pending_frames),
            active_frame: ActiveFrame {
                query_pools: RwLock::new(PendingFramePools::default()),
                closed_scope_sender,
                closed_scope_receiver: Mutex::new(closed_scope_receiver),
            },

            num_open_scopes: AtomicU32::new(0),
            next_scope_handle: AtomicU32::new(0),

            size_for_new_query_pools: QueryPool::MIN_CAPACITY,

            settings,

            #[cfg(feature = "tracy")]
            tracy_context: None,
        })
    }

    /// Creates a new profiler and connects to a running Tracy client.
    #[cfg(feature = "tracy")]
    pub fn new_with_tracy_client(
        settings: GpuProfilerSettings,
        backend: wgpu::Backend,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<Self, CreationError> {
        let mut profiler = Self::new(settings)?;
        profiler.tracy_context = Some(tracy::create_tracy_gpu_client(backend, device, queue)?);
        Ok(profiler)
    }

    /// Changes the settings of an existing profiler.
    ///
    /// If timer scopes are disabled (by setting [GpuProfilerSettings::enable_timer_scopes] to false),
    /// any timer queries that are in flight will still be processed,
    /// but unused query sets and buffers will be deallocated during [`Self::process_finished_frame`].
    pub fn change_settings(&mut self, settings: GpuProfilerSettings) -> Result<(), SettingsError> {
        if self.num_open_scopes.load(Ordering::Acquire) > 0 {
            // TODO: we can allow this now, borrow checker takes care of all remaining issues.
            Err(SettingsError::HasOpenScopes)
        } else {
            settings.validate()?;
            if !settings.enable_timer_scopes {
                self.unused_pools.clear();
            }
            self.settings = settings;

            Ok(())
        }
    }

    /// Starts a new auto-closing profiler scope.
    ///
    /// To nest scopes inside this scope, call [`Scope::scope`] on the returned scope.
    ///
    /// If an [`wgpu::CommandEncoder`] is passed but the [`wgpu::Device`]
    /// does not support [`wgpu::Features::TIMESTAMP_QUERY`], no gpu timer will be queried and the scope will
    /// not show up in the final results.
    /// If an [`wgpu::ComputePass`] or [`wgpu::RenderPass`] is passed but the [`wgpu::Device`]
    /// does not support [`wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES`], no scope will be opened.
    ///
    /// If [`GpuProfilerSettings::enable_debug_groups`] is true, a debug group will be pushed on the encoder or pass.
    ///
    /// Scope is automatically closed on drop.
    #[must_use]
    #[track_caller]
    #[inline]
    pub fn scope<'a, Recorder: ProfilerCommandRecorder>(
        &'a self,
        label: impl Into<String>,
        encoder_or_pass: &'a mut Recorder,
        device: &wgpu::Device,
    ) -> Scope<'a, Recorder> {
        let scope = self.begin_scope(label, encoder_or_pass, device);
        Scope {
            profiler: self,
            recorder: encoder_or_pass,
            scope: Some(scope),
        }
    }

    /// Starts a new auto-closing profiler scope that takes ownership of the passed encoder or rendering/compute pass.
    ///
    /// To nest scopes inside this scope, call [`OwningScope::scope`] on the returned scope.
    ///
    /// If an [`wgpu::CommandEncoder`] is passed but the [`wgpu::Device`]
    /// does not support [`wgpu::Features::TIMESTAMP_QUERY`], no gpu timer will be queried and the scope will
    /// not show up in the final results.
    /// If an [`wgpu::ComputePass`] or [`wgpu::RenderPass`] is passed but the [`wgpu::Device`]
    /// does not support [`wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES`], no scope will be opened.
    ///
    /// If [`GpuProfilerSettings::enable_debug_groups`] is true, a debug group will be pushed on the encoder or pass.
    ///
    /// Scope is automatically closed on drop.
    #[must_use]
    #[track_caller]
    #[inline]
    pub fn owning_scope<'a, Recorder: ProfilerCommandRecorder>(
        &'a self,
        label: impl Into<String>,
        mut encoder_or_pass: Recorder,
        device: &wgpu::Device,
    ) -> OwningScope<'a, Recorder> {
        let scope = self.begin_scope(label, &mut encoder_or_pass, device);
        OwningScope {
            profiler: self,
            recorder: encoder_or_pass,
            scope: Some(scope),
        }
    }

    /// Starts a new **manually closed** profiler scope that takes ownership of the passed encoder or rendering/compute pass.
    ///
    /// Does NOT call [`GpuProfiler::end_scope()`] on drop.
    /// This construct is just for completeness in cases where working with scopes is preferred but one can't rely on the Drop call in the right place.
    /// This is useful when the owned value needs to be recovered after the end of the scope.
    /// In particular, to submit a [`wgpu::CommandEncoder`] to a queue, ownership of the encoder is necessary.
    ///
    /// To nest scopes inside this scope, call [`ManualOwningScope::scope`] on the returned scope.
    ///
    /// If an [`wgpu::CommandEncoder`] is passed but the [`wgpu::Device`]
    /// does not support [`wgpu::Features::TIMESTAMP_QUERY`], no gpu timer will be queried and the scope will
    /// not show up in the final results.
    /// If an [`wgpu::ComputePass`] or [`wgpu::RenderPass`] is passed but the [`wgpu::Device`]
    /// does not support [`wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES`], no scope will be opened.
    ///
    /// If [`GpuProfilerSettings::enable_debug_groups`] is true, a debug group will be pushed on the encoder or pass.
    #[must_use]
    #[track_caller]
    #[inline]
    pub fn manual_owning_scope<'a, Recorder: ProfilerCommandRecorder>(
        &'a self,
        label: impl Into<String>,
        mut encoder_or_pass: Recorder,
        device: &wgpu::Device,
    ) -> ManualOwningScope<'a, Recorder> {
        let scope = self.begin_scope(label, &mut encoder_or_pass, device);
        ManualOwningScope {
            profiler: self,
            recorder: encoder_or_pass,
            scope: Some(scope),
        }
    }

    /// Starts a new profiler scope on a given encoder or rendering/compute pass if enabled that must be manually closed.
    ///
    /// The returned scope *must* be closed by calling [`GpuProfiler::end_scope`].
    /// Dropping it without closing it will trigger a debug assertion.
    /// To do this automatically, use `GpuProfiler::scope`/`GpuProfiler::owning_scope` instead.
    ///
    /// If an [`wgpu::CommandEncoder`] is passed but the [`wgpu::Device`]
    /// does not support [`wgpu::Features::TIMESTAMP_QUERY`], no gpu timer will be queried and the scope will
    /// not show up in the final results.
    /// If an [`wgpu::ComputePass`] or [`wgpu::RenderPass`] is passed but the [`wgpu::Device`]
    /// does not support [`wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES`], no scope will be opened.
    ///
    /// If [`GpuProfilerSettings::enable_debug_groups`] is true, a debug group will be pushed on the encoder or pass.
    #[track_caller]
    #[must_use]
    pub fn begin_scope<Recorder: ProfilerCommandRecorder>(
        &self,
        label: impl Into<String>,
        encoder_or_pass: &mut Recorder,
        device: &wgpu::Device,
    ) -> GpuTimerScope {
        let mut scope = self.begin_scope_internal(label.into(), encoder_or_pass, device);
        if let Some(query) = &mut scope.query {
            encoder_or_pass.write_timestamp(&query.pool.query_set, query.start_query_idx);
            query.usage_state = QueryPairUsageState::OnlyStartWritten;
        };

        if self.settings.enable_debug_groups {
            encoder_or_pass.push_debug_group(&scope.label);
            scope.has_debug_group = true;
        }
        scope
    }

    /// Need to call end scope with the encoder again, not the pass the scope is used with.
    /// TODO: proper doc
    /// TODO: highlevel methods for this?
    /// TODO: Naming needs a facelift - `GpuTimerScope` vs `Scope` is WEIRD!
    pub fn begin_pass_scope(
        &self,
        label: impl Into<String>,
        encoder: &mut wgpu::CommandEncoder,
        device: &wgpu::Device,
    ) -> GpuTimerScope {
        let mut scope = self.begin_scope_internal(label.into(), encoder, device);
        if let Some(query) = &mut scope.query {
            query.usage_state = QueryPairUsageState::ReservedForPassTimestampWrites;
        }
        scope
    }

    /// Ends passed scope.
    ///
    /// Behavior is not defined if the last open scope was opened on a different encoder or pass than the one passed here.
    ///
    /// If the previous call to [`GpuProfiler::begin_scope`] did not open a timer scope because it was not supported or disabled,
    /// this call will do nothing (except closing the currently open debug scope if enabled).
    pub fn end_scope<Recorder: ProfilerCommandRecorder>(
        &self,
        encoder_or_pass: &mut Recorder,
        mut scope: GpuTimerScope,
    ) {
        if let Some(query) = &mut scope.query {
            match query.usage_state {
                QueryPairUsageState::Reserved => {
                    unreachable!("Scope has been reserved but isn't used for anything!")
                }
                QueryPairUsageState::ReservedForPassTimestampWrites => {
                    // No need to do a timestamp write, this is handled by wgpu.
                }
                QueryPairUsageState::OnlyStartWritten => {
                    encoder_or_pass
                        .write_timestamp(&query.pool.query_set, query.start_query_idx + 1);
                    query.usage_state = QueryPairUsageState::BothStartAndEndWritten;
                }
                QueryPairUsageState::BothStartAndEndWritten => {
                    unreachable!("Scope has already been closed!")
                }
            }

            #[cfg(feature = "tracy")]
            if let Some(ref mut tracy_scope) = scope.tracy_scope {
                tracy_scope.end_zone();
            }
        }

        if scope.has_debug_group {
            encoder_or_pass.pop_debug_group();
        }

        let send_result = self.active_frame.closed_scope_sender.send(scope);

        // The only way we can fail sending the scope is if the receiver has been dropped.
        // Since it sits on `active_frame` as well, there's no way for this to happen!
        debug_assert!(send_result.is_ok());

        // Count scopes even if we haven't processed this one, makes experiences more consistent
        // if there's a lack of support for some queries.
        self.num_open_scopes.fetch_sub(1, Ordering::Release);
    }

    /// Puts query resolve commands in the encoder for all unresolved, pending queries of the active profiler frame.
    ///
    /// Note that you do *not* need to do this for every encoder, it is sufficient do do this once per frame as long
    /// as you submit the corresponding command buffer after all others that may have opened scopes in the same frame.
    /// (It does not matter if the passed encoder itself has previously opened scopes or not.)
    /// If you were to make this part of a command buffer that is enqueued before any other that has
    /// opened scopes in the same profiling frame, no failure will occur but some timing results may be invalid.
    ///
    /// It is advised to call this only once at the end of a profiling frame, but it is safe to do so several times.
    ///
    ///
    /// Implementation note:
    /// This method could be made `&self`, taking the internal lock on the query pools.
    /// However, the intended use is to call this once at the end of a frame, so we instead
    /// encourage this explicit sync point and avoid the lock.
    pub fn resolve_queries(&mut self, encoder: &mut wgpu::CommandEncoder) {
        let query_pools = self.active_frame.query_pools.get_mut();

        for query_pool in query_pools.used_pools.iter_mut() {
            // We sync with the last update of num_used_query (which has Release semantics)
            // mostly to be on the safe side - it happened inside a lock which gives it release semantics anyways
            // but the concern is that if we don't acquire here, we may miss on other side prior effects of the query begin.
            let num_used_queries = query_pool.num_used_queries.load(Ordering::Acquire);
            let num_resolved_queries = query_pool.num_resolved_queries.load(Ordering::Acquire);

            if num_resolved_queries == num_used_queries {
                continue;
            }

            assert!(num_resolved_queries < num_used_queries);

            encoder.resolve_query_set(
                &query_pool.query_set,
                num_resolved_queries..num_used_queries,
                &query_pool.resolve_buffer,
                (num_resolved_queries * QUERY_SIZE) as u64,
            );
            query_pool
                .num_resolved_queries
                .store(num_used_queries, Ordering::Release);

            encoder.copy_buffer_to_buffer(
                &query_pool.resolve_buffer,
                0,
                &query_pool.read_buffer,
                0,
                (num_used_queries * QUERY_SIZE) as u64,
            );
        }
    }

    /// Marks the end of a frame.
    ///
    /// Needs to be called **after** submitting any encoder used in the current profiler frame.
    ///
    /// Fails if there are still open scopes or unresolved queries.
    pub fn end_frame(&mut self) -> Result<(), EndFrameError> {
        let num_open_scopes = self.num_open_scopes.load(Ordering::Acquire);
        if num_open_scopes != 0 {
            return Err(EndFrameError::UnclosedScopes(num_open_scopes));
        }

        let query_pools = self.active_frame.query_pools.get_mut();

        let mut new_pending_frame = PendingFrame {
            query_pools: std::mem::take(&mut query_pools.used_pools),
            closed_scope_by_parent_handle: HashMap::new(),
            mapped_buffers: Arc::new(AtomicU32::new(0)),
        };

        for scope in self.active_frame.closed_scope_receiver.get_mut().try_iter() {
            new_pending_frame
                .closed_scope_by_parent_handle
                .entry(scope.parent_handle)
                .or_default()
                .push(scope);
        }

        // All loads of pool.num_used_queries are Relaxed since we assume,
        // that we already acquired the state during `resolve_queries` and no further otherwise unobserved
        // modifications happened since then.

        let num_unresolved_queries = new_pending_frame
            .query_pools
            .iter()
            .map(|pool| {
                pool.num_used_queries.load(Ordering::Relaxed)
                    - pool.num_resolved_queries.load(Ordering::Relaxed)
            })
            .sum();
        if num_unresolved_queries != 0 {
            return Err(EndFrameError::UnresolvedQueries(num_unresolved_queries));
        }

        // Next time we create a new query pool, we want it to be at least as big to hold all queries of this frame.
        self.size_for_new_query_pools = self
            .size_for_new_query_pools
            .max(
                new_pending_frame
                    .query_pools
                    .iter()
                    .map(|pool| pool.num_used_queries.load(Ordering::Relaxed))
                    .sum(),
            )
            .min(QUERY_SET_MAX_QUERIES);

        // Make sure we don't overflow.
        if self.pending_frames.len() == self.settings.max_num_pending_frames {
            // Drop previous (!) frame.
            // Dropping the oldest frame could get us into an endless cycle where we're never able to complete
            // any pending frames as the ones closest to completion would be evicted.
            if let Some(dropped_frame) = self.pending_frames.pop() {
                // Drop scopes first since they still have references to the query pools that we want to reuse.
                drop(dropped_frame.closed_scope_by_parent_handle);

                // Mark the frame as dropped. We'll give back the query pools once the mapping is done.
                // Any previously issued map_async call that haven't finished yet, will invoke their callback with mapping abort.
                self.reset_and_cache_unused_query_pools(dropped_frame.query_pools);
            }
        }

        // Map all buffers.
        for pool in new_pending_frame.query_pools.iter_mut() {
            let mapped_buffers = new_pending_frame.mapped_buffers.clone();
            pool.read_buffer
                .slice(0..(pool.num_used_queries.load(Ordering::Relaxed) * QUERY_SIZE) as u64)
                .map_async(wgpu::MapMode::Read, move |mapping_result| {
                    // Mapping should not fail unless it was cancelled due to the frame being dropped.
                    match mapping_result {
                        Err(_) => {
                            // We only want to ignore the error iff the mapping has been aborted by us (due to a dropped frame, see above).
                            // In any other case, we need should panic as this would imply something went seriously sideways.
                            //
                            // As of writing, this is not yet possible in wgpu, see https://github.com/gfx-rs/wgpu/pull/2939
                        }
                        Ok(()) => {
                            mapped_buffers.fetch_add(1, std::sync::atomic::Ordering::Release);
                        }
                    }
                });
        }

        // Enqueue
        self.pending_frames.push(new_pending_frame);
        assert!(self.pending_frames.len() <= self.settings.max_num_pending_frames);

        Ok(())
    }

    /// Checks if all timer queries for the oldest pending finished frame are done and returns that snapshot if any.
    ///
    /// timestamp_period:
    ///    The timestamp period of the device. Pass the result of [`wgpu::Queue::get_timestamp_period()`].
    ///    Note that some implementations (Chrome as of writing) may converge to a timestamp period while the application is running,
    ///    so caching this value is usually not recommended.
    pub fn process_finished_frame(
        &mut self,
        timestamp_period: f32,
    ) -> Option<Vec<GpuTimerScopeResult>> {
        let frame = self.pending_frames.first_mut()?;

        // We only process if all mappings succeed.
        if frame
            .mapped_buffers
            .load(std::sync::atomic::Ordering::Acquire)
            != frame.query_pools.len() as u32
        {
            return None;
        }

        let mut frame = self.pending_frames.remove(0);

        let results = {
            let timestamp_to_sec = timestamp_period as f64 / 1000.0 / 1000.0 / 1000.0;

            Self::process_timings_recursive(
                timestamp_to_sec,
                &mut frame.closed_scope_by_parent_handle,
                ROOT_SCOPE_HANDLE,
            )
        };

        self.reset_and_cache_unused_query_pools(frame.query_pools);

        Some(results)
    }
}

// --------------------------------------------------------------------------------
// Internals
// --------------------------------------------------------------------------------

const QUERY_SIZE: u32 = wgpu::QUERY_SIZE;
const QUERY_SET_MAX_QUERIES: u32 = wgpu::QUERY_SET_MAX_QUERIES;

/// Returns true if a timestamp should be written to the encoder or pass.
fn timestamp_write_supported<Recorder: ProfilerCommandRecorder>(
    encoder_or_pass: &mut Recorder,
    features: wgpu::Features,
) -> bool {
    let required_feature = if encoder_or_pass.is_pass() {
        wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES
    } else {
        wgpu::Features::TIMESTAMP_QUERY
    };
    features.contains(required_feature)
}

impl GpuProfiler {
    fn next_scope_tree_handle(&self) -> GpuTimerScopeTreeHandle {
        // Relaxed is fine, we just want a number that nobody uses this frame already.
        let mut handle = self.next_scope_handle.fetch_add(1, Ordering::Relaxed);

        // We don't ever expect to run out of handles during a single frame, but who knows how long the app runs.
        while handle == ROOT_SCOPE_HANDLE {
            handle = self.next_scope_handle.fetch_add(1, Ordering::Relaxed);
        }

        handle
    }

    fn reset_and_cache_unused_query_pools(&mut self, mut discarded_pools: Vec<Arc<QueryPool>>) {
        let capacity_threshold = self.size_for_new_query_pools / 2;
        for pool in discarded_pools.drain(..) {
            // If the pool is truly unused now, it's ref count should be 1!
            // If we use it anywhere else we have an implementation bug.
            let mut pool = Arc::into_inner(pool).expect("Pool still in use");
            pool.reset();

            // If a pool was less than half of the size of the max frame, then we don't keep it.
            // This way we're going to need less pools in upcoming frames and thus have less overhead in the long run.
            // If timer scopes were disabled, we also don't keep any pools.
            if self.settings.enable_timer_scopes && pool.capacity >= capacity_threshold {
                self.active_frame
                    .query_pools
                    .get_mut()
                    .unused_pools
                    .push(pool);
            }
        }
    }

    fn try_reserve_query_pair(pool: &Arc<QueryPool>) -> Option<ReservedQueryPair> {
        let mut num_used_queries = pool.num_used_queries.load(Ordering::Relaxed);

        loop {
            if pool.capacity < num_used_queries + 2 {
                // This pool is out of capacity, we failed the operation.
                return None;
            }

            match pool.num_used_queries.compare_exchange_weak(
                num_used_queries,
                num_used_queries + 2,
                // Write to num_used_queries with release semantics to be on the safe side.
                // (It doesn't look like there's other side effects that we need to publish.)
                Ordering::Release,
                // No barrier for the failure case.
                // The only thing we have to acquire is the pool's capacity which is constant and
                // was definitely acquired by the RWLock prior to this call.
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    // We successfully acquired two queries!
                    return Some(ReservedQueryPair {
                        pool: pool.clone(),
                        start_query_idx: num_used_queries,
                        usage_state: QueryPairUsageState::Reserved,
                    });
                }
                Err(updated) => {
                    // Someone else acquired queries in the meantime, try again.
                    num_used_queries = updated;
                }
            }
        }
    }

    // Reserves two query objects.
    // Our query pools always have an even number of queries, so we know the next query is the next in the same pool.
    fn reserve_query_pair(&self, device: &wgpu::Device) -> ReservedQueryPair {
        // First, try to allocate from current top pool.
        // Requires taking a read lock on the current query pool.
        {
            let query_pools = self.active_frame.query_pools.read();
            if let Some(pair) = query_pools
                .used_pools
                .last()
                .and_then(Self::try_reserve_query_pair)
            {
                return pair;
            }
        }
        // If this didn't work, we may need to add a new pool.
        // Requires taking a write lock on the current query pool.
        {
            let mut query_pools = self.active_frame.query_pools.write();

            // It could be that by now, another thread has already added a new pool!
            // This is a bit unfortunate because it means we unnecessarily took a write lock, but it seems hard to get around this.
            if let Some(pair) = query_pools
                .used_pools
                .last()
                .and_then(Self::try_reserve_query_pair)
            {
                return pair;
            }

            // Now we know for certain that the last pool is exhausted, so add a new one!
            let new_pool = if let Some(reused_pool) = query_pools.unused_pools.pop() {
                // First check if there's an unused pool we can take.
                Arc::new(reused_pool)
            } else {
                // If we can't, create a new pool that is as big as all previous pools combined.
                Arc::new(QueryPool::new(
                    query_pools
                        .used_pools
                        .iter()
                        .map(|pool| pool.capacity)
                        .sum::<u32>()
                        .max(self.size_for_new_query_pools)
                        .min(QUERY_SET_MAX_QUERIES),
                    device,
                ))
            };

            let pair = Self::try_reserve_query_pair(&new_pool)
                .expect("Freshly reserved pool doesn't have enough capacity");
            query_pools.used_pools.push(new_pool);

            pair
        }
    }

    #[track_caller]
    #[must_use]
    fn begin_scope_internal<Recorder: ProfilerCommandRecorder>(
        &self,
        label: String,
        encoder_or_pass: &mut Recorder,
        device: &wgpu::Device,
    ) -> GpuTimerScope {
        // Give opening/closing scopes acquire/release semantics:
        // This way, we won't get any nasty surprises when observing zero open scopes.
        self.num_open_scopes.fetch_add(1, Ordering::Acquire);

        let handle = self.next_scope_tree_handle();

        let (query, _tracy_scope) = if self.settings.enable_timer_scopes
            && timestamp_write_supported(encoder_or_pass, device.features())
        {
            #[cfg(feature = "tracy")]
            let tracy_scope = {
                let location = std::panic::Location::caller();
                self.tracy_context.as_ref().and_then(|c| {
                    c.span_alloc(&label, "", location.file(), location.line())
                        .ok()
                })
            };
            #[cfg(not(feature = "tracy"))]
            let tracy_scope = Option::<()>::None;

            (Some(self.reserve_query_pair(device)), tracy_scope)
        } else {
            (None, None)
        };

        GpuTimerScope {
            label,
            pid: std::process::id(),
            tid: std::thread::current().id(),
            query,
            handle,
            parent_handle: ROOT_SCOPE_HANDLE,
            has_debug_group: false,
            #[cfg(feature = "tracy")]
            tracy_scope: _tracy_scope,
        }
    }

    fn process_timings_recursive(
        timestamp_to_sec: f64,
        closed_scope_by_parent_handle: &mut HashMap<GpuTimerScopeTreeHandle, Vec<GpuTimerScope>>,
        parent_handle: GpuTimerScopeTreeHandle,
    ) -> Vec<GpuTimerScopeResult> {
        let Some(scopes_with_same_parent) = closed_scope_by_parent_handle.remove(&parent_handle)
        else {
            return Vec::new();
        };

        scopes_with_same_parent
            .into_iter()
            .filter_map(|mut scope| {
                let Some(query) = scope.query.take() else {
                    // Inactive scopes don't have any results or nested scopes with results.
                    // Currently, we drop them from the results completely.
                    // In the future we could still make them show up since they convey information like label & pid/tid.
                    return None;
                };

                // Read timestamp from buffer.
                // By design timestamps for start/end are consecutive.
                let offset = (query.start_query_idx * QUERY_SIZE) as u64;
                let buffer_slice = &query
                    .pool
                    .read_buffer
                    .slice(offset..(offset + (QUERY_SIZE * 2) as u64))
                    .get_mapped_range();
                let start_raw =
                    u64::from_le_bytes(buffer_slice[0..QUERY_SIZE as usize].try_into().unwrap());
                let end_raw = u64::from_le_bytes(
                    buffer_slice[QUERY_SIZE as usize..(QUERY_SIZE as usize) * 2]
                        .try_into()
                        .unwrap(),
                );

                #[cfg(feature = "tracy")]
                if let Some(tracy_scope) = scope.tracy_scope.take() {
                    tracy_scope.upload_timestamp(start_raw as i64, end_raw as i64);
                }

                let nested_scopes = Self::process_timings_recursive(
                    timestamp_to_sec,
                    closed_scope_by_parent_handle,
                    scope.handle,
                );

                Some(GpuTimerScopeResult {
                    label: std::mem::take(&mut scope.label),
                    time: (start_raw as f64 * timestamp_to_sec)
                        ..(end_raw as f64 * timestamp_to_sec),
                    nested_scopes,
                    pid: scope.pid,
                    tid: scope.tid,
                })
            })
            .collect::<Vec<_>>()
    }
}

#[derive(PartialEq, Eq)]
enum QueryPairUsageState {
    /// Transitional state used upon creation.
    Reserved,

    /// Don't do manual timestamp writes, wgpu is expected to do them for us.
    ReservedForPassTimestampWrites,

    /// Start query has been used, end query is still available.
    OnlyStartWritten,

    /// Both start & end query have been used.
    BothStartAndEndWritten,
}

struct ReservedQueryPair {
    /// QueryPool on which both start & end queries of the scope are done.
    ///
    /// By putting an arc here instead of an index into a vec, we don't need
    /// need to take any locks upon closing a profiling scope.
    pool: Arc<QueryPool>,

    /// Query index at which the scope begins.
    /// The query after this is reserved for the end of the scope.
    start_query_idx: u32,

    /// Current use of the query pair.
    usage_state: QueryPairUsageState,
}

/// A pool of queries, consisting of a single queryset & buffer for query results.
#[derive(Debug)]
struct QueryPool {
    query_set: wgpu::QuerySet,

    resolve_buffer: wgpu::Buffer,
    read_buffer: wgpu::Buffer,

    capacity: u32,
    num_used_queries: AtomicU32,
    num_resolved_queries: AtomicU32,
}

impl QueryPool {
    const MIN_CAPACITY: u32 = 32;

    fn new(capacity: u32, device: &wgpu::Device) -> Self {
        QueryPool {
            query_set: device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("GpuProfiler - Query Set"),
                ty: wgpu::QueryType::Timestamp,
                count: capacity,
            }),

            resolve_buffer: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("GpuProfiler - Query Resolve Buffer"),
                size: (QUERY_SIZE * capacity) as u64,
                usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            }),

            read_buffer: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("GpuProfiler - Query Read Buffer"),
                size: (QUERY_SIZE * capacity) as u64,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            }),

            capacity,
            num_used_queries: AtomicU32::new(0),
            num_resolved_queries: AtomicU32::new(0),
        }
    }

    fn reset(&mut self) {
        self.num_used_queries = AtomicU32::new(0);
        self.num_resolved_queries = AtomicU32::new(0);
        self.read_buffer.unmap();
    }
}

#[derive(Default)]
struct PendingFramePools {
    /// List of all pools used in this frame.
    /// The last pool is the one new profiling scopes will try to make timer queries into.
    used_pools: Vec<Arc<QueryPool>>,

    /// List of unused pools recycled from previous frames.
    unused_pools: Vec<QueryPool>,
}

/// Internal handle to building a tree of profiling scopes.
type GpuTimerScopeTreeHandle = u32;

/// Handle for the root scope.
const ROOT_SCOPE_HANDLE: GpuTimerScopeTreeHandle = std::u32::MAX;

struct ActiveFrame {
    query_pools: RwLock<PendingFramePools>,

    /// Closed scopes get send to this channel.
    ///
    /// Note that channel is still overkill for what we want here:
    /// We're in a multi producer situation, *but* the single consumer is known to be only
    /// active in a mut context, i.e. while we're consuming we know that we're not producing.
    /// We have to wrap it in a Mutex because the channel is not Sync, but we actually never lock it
    /// since we only ever access it in a `mut` context.
    closed_scope_sender: std::sync::mpsc::Sender<GpuTimerScope>,
    closed_scope_receiver: Mutex<std::sync::mpsc::Receiver<GpuTimerScope>>,
}

struct PendingFrame {
    query_pools: Vec<Arc<QueryPool>>,
    closed_scope_by_parent_handle: HashMap<GpuTimerScopeTreeHandle, Vec<GpuTimerScope>>,

    /// Keeps track of the number of buffers in the query pool that have been mapped successfully.
    mapped_buffers: std::sync::Arc<std::sync::atomic::AtomicU32>,
}

pub trait ProfilerCommandRecorder {
    /// Returns `true` if it's a pass or `false` if it's an encoder
    fn is_pass(&self) -> bool;
    fn write_timestamp(&mut self, query_set: &wgpu::QuerySet, query_index: u32);
    fn push_debug_group(&mut self, label: &str);
    fn pop_debug_group(&mut self);
}

macro_rules! ImplProfilerCommandRecorder {
    ($($name:ident $(< $lt:lifetime >)? : $pass:literal,)*) => {
        $(
            impl $(< $lt >)? ProfilerCommandRecorder for wgpu::$name $(< $lt >)? {
                fn is_pass(&self) -> bool { $pass }

                fn write_timestamp(&mut self, query_set: &wgpu::QuerySet, query_index: u32) {
                    self.write_timestamp(query_set, query_index)
                }

                fn push_debug_group(&mut self, label: &str) {
                    self.push_debug_group(label)
                }

                fn pop_debug_group(&mut self) {
                    self.pop_debug_group()
                }
            }
        )*
    };
}

ImplProfilerCommandRecorder!(CommandEncoder:false, RenderPass<'a>:true, ComputePass<'a>:true,);
