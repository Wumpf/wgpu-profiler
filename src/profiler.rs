use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
};

use parking_lot::{Mutex, RwLock};

use crate::{
    CreationError, EndFrameError, GpuProfilerQuery, GpuProfilerSettings, GpuTimerQueryResult,
    ManualOwningScope, OwningScope, ProfilerCommandRecorder, Scope, SettingsError,
};

/// Profiler instance.
///
/// You can have an arbitrary number of independent profiler instances per application/adapter.
/// Manages all the necessary [`wgpu::QuerySet`] and [`wgpu::Buffer`] behind the scenes.
///
/// Any query creation method may allocate a new [`wgpu::QuerySet`] and [`wgpu::Buffer`] internally if necessary.
///
/// [`GpuProfiler`] is associated with a single [`wgpu::Device`] upon creation.
/// All references wgpu objects passed in subsequent calls must originate from that device.
pub struct GpuProfiler {
    device: wgpu::Device,

    unused_pools: Vec<QueryPool>,

    active_frame: ActiveFrame,
    pending_frames: Vec<PendingFrame>,

    num_open_queries: AtomicU32,
    next_query_handle: AtomicU32,

    size_for_new_query_pools: u32,

    settings: GpuProfilerSettings,

    #[cfg(feature = "tracy")]
    tracy_context: Option<tracy_client::GpuContext>,
}

// Public interface
impl GpuProfiler {
    /// Combination of all timer query features [`GpuProfiler`] can leverage.
    pub const ALL_WGPU_TIMER_FEATURES: wgpu::Features = wgpu::Features::TIMESTAMP_QUERY
        .union(wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS)
        .union(wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES);

    /// Combination of all timer query features [`GpuProfiler`] can leverage.
    #[deprecated(since = "0.9.0", note = "Use ALL_WGPU_TIMER_FEATURES instead")]
    pub const REQUIRED_WGPU_FEATURES: wgpu::Features = GpuProfiler::ALL_WGPU_TIMER_FEATURES;

    /// Creates a new Profiler object.
    ///
    /// There is nothing preventing the use of several independent profiler objects.
    pub fn new(
        device: &wgpu::Device,
        settings: GpuProfilerSettings,
    ) -> Result<Self, CreationError> {
        settings.validate()?;

        let (closed_scope_sender, closed_scope_receiver) = std::sync::mpsc::channel();

        Ok(GpuProfiler {
            device: device.clone(),

            unused_pools: Vec::new(),

            pending_frames: Vec::with_capacity(settings.max_num_pending_frames),
            active_frame: ActiveFrame {
                query_pools: RwLock::new(PendingFramePools::default()),
                closed_query_sender: closed_scope_sender,
                closed_query_receiver: Mutex::new(closed_scope_receiver),
            },

            num_open_queries: AtomicU32::new(0),
            next_query_handle: AtomicU32::new(0),

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
        let mut profiler = Self::new(device, settings)?;
        profiler.tracy_context = Some(crate::tracy::create_tracy_gpu_client(
            backend, device, queue,
        )?);
        Ok(profiler)
    }

    /// Returns currently active settings.
    pub fn settings(&self) -> &GpuProfilerSettings {
        &self.settings
    }

    /// Changes the settings of an existing profiler.
    ///
    /// If timer scopes are disabled by setting [`GpuProfilerSettings::enable_timer_queries`] to false,
    /// any timer queries that are in flight will still be processed,
    /// but unused query sets and buffers will be deallocated during [`Self::process_finished_frame`].
    /// Similarly, any opened debugging scope will still be closed if debug groups are disabled by setting
    /// [`GpuProfilerSettings::enable_debug_groups`] to false.
    pub fn change_settings(&mut self, settings: GpuProfilerSettings) -> Result<(), SettingsError> {
        settings.validate()?;
        if !settings.enable_timer_queries {
            self.unused_pools.clear();
        }
        self.settings = settings;

        Ok(())
    }

    /// Starts a new auto-closing profiler scope.
    ///
    /// To nest scopes inside this scope, call [`Scope::scope`] on the returned scope.
    ///
    /// If an [`wgpu::CommandEncoder`] is passed but the [`wgpu::Device`]
    /// does not support [`wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS`], no gpu timer will
    /// be queried and the scope will not show up in the final results.
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
    ) -> Scope<'a, Recorder> {
        let scope = self.begin_query(label, encoder_or_pass);
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
    /// does not support [`wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS`], no gpu timer will be queried
    /// and the scope will not show up in the final results.
    /// If an [`wgpu::ComputePass`] or [`wgpu::RenderPass`] is passed but the [`wgpu::Device`]
    /// does not support [`wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES`], no scope will be opened.
    ///
    /// If [`GpuProfilerSettings::enable_debug_groups`] is true, a debug group will be pushed on the encoder or pass.
    ///
    /// Scope is automatically closed on drop.
    #[must_use]
    #[track_caller]
    #[inline]
    pub fn owning_scope<Recorder: ProfilerCommandRecorder>(
        &'_ self,
        label: impl Into<String>,
        mut encoder_or_pass: Recorder,
    ) -> OwningScope<'_, Recorder> {
        let scope = self.begin_query(label, &mut encoder_or_pass);
        OwningScope {
            profiler: self,
            recorder: encoder_or_pass,
            scope: Some(scope),
        }
    }

    /// Starts a new **manually closed** profiler scope that takes ownership of the passed encoder or rendering/compute pass.
    ///
    /// Does NOT call [`GpuProfiler::end_query()`] on drop.
    /// This construct is just for completeness in cases where working with scopes is preferred but one can't rely on the Drop call in the right place.
    /// This is useful when the owned value needs to be recovered after the end of the scope.
    /// In particular, to submit a [`wgpu::CommandEncoder`] to a queue, ownership of the encoder is necessary.
    ///
    /// To nest scopes inside this scope, call [`ManualOwningScope::scope`] on the returned scope.
    ///
    /// If an [`wgpu::CommandEncoder`] is passed but the [`wgpu::Device`]
    /// does not support [`wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS`], no gpu timer will be queried and the scope will
    /// not show up in the final results.
    /// If an [`wgpu::ComputePass`] or [`wgpu::RenderPass`] is passed but the [`wgpu::Device`]
    /// does not support [`wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES`], no scope will be opened.
    ///
    /// If [`GpuProfilerSettings::enable_debug_groups`] is true, a debug group will be pushed on the encoder or pass.
    #[must_use]
    #[track_caller]
    #[inline]
    pub fn manual_owning_scope<Recorder: ProfilerCommandRecorder>(
        &self,
        label: impl Into<String>,
        mut encoder_or_pass: Recorder,
    ) -> ManualOwningScope<'_, Recorder> {
        let scope = self.begin_query(label, &mut encoder_or_pass);
        ManualOwningScope {
            profiler: self,
            recorder: encoder_or_pass,
            scope: Some(scope),
        }
    }

    /// Starts a new profiler query on the given encoder or rendering/compute pass (if enabled).
    ///
    /// The returned query *must* be closed by calling [`GpuProfiler::end_query`] with the same encoder/pass,
    /// even if timer queries are disabled.
    /// To do this automatically, use [`GpuProfiler::scope`]/[`GpuProfiler::owning_scope`] instead.
    ///
    /// If an [`wgpu::CommandEncoder`] is passed but the [`wgpu::Device`]
    /// does not support [`wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS`], no gpu timer will be queried and the scope will
    /// not show up in the final results.
    /// If an [`wgpu::ComputePass`] or [`wgpu::RenderPass`] is passed but the [`wgpu::Device`]
    /// does not support [`wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES`], no timer queries will be allocated.
    ///
    /// If [`GpuProfilerSettings::enable_debug_groups`] is true, a debug group will be pushed on the encoder or pass.
    #[track_caller]
    #[must_use]
    pub fn begin_query<Recorder: ProfilerCommandRecorder>(
        &self,
        label: impl Into<String>,
        encoder_or_pass: &mut Recorder,
    ) -> GpuProfilerQuery {
        let is_for_pass_timestamp_writes = false;
        let mut query =
            self.begin_query_internal(label.into(), is_for_pass_timestamp_writes, encoder_or_pass);
        if let Some(timer_query) = &mut query.timer_query_pair {
            encoder_or_pass
                .write_timestamp(&timer_query.pool.query_set, timer_query.start_query_idx);
            timer_query.usage_state = QueryPairUsageState::OnlyStartWritten;
        };

        if self.settings.enable_debug_groups {
            encoder_or_pass.push_debug_group(&query.label);
            query.has_debug_group = true;
        }
        query
    }

    /// Starts a new profiler query to be used for render/compute pass timestamp writes.
    ///
    /// The returned query *must* be closed by calling [`GpuProfiler::end_query`], even if timer queries are disabled.
    /// To do this automatically, use [`Scope::scoped_render_pass`]/[`Scope::scoped_compute_pass`] instead.
    ///
    /// Call [`GpuProfilerQuery::render_pass_timestamp_writes`] or [`GpuProfilerQuery::compute_pass_timestamp_writes`]
    /// to acquire the corresponding [`wgpu::RenderPassTimestampWrites`]/[`wgpu::ComputePassTimestampWrites`] object.
    ///
    /// If the [`wgpu::Device`] does not support [`wgpu::Features::TIMESTAMP_QUERY`], no gpu timer will be reserved.
    ///
    /// Unlike [`GpuProfiler::begin_query`] this will not create a debug scope,
    /// in order to not force passing of the same encoder/pass to [`GpuProfiler::end_query`].
    /// (this is needed to relax resource tracking requirements a bit, making it easier to implement the automatic scopes)
    pub fn begin_pass_query(
        &self,
        label: impl Into<String>,
        encoder: &mut wgpu::CommandEncoder,
    ) -> GpuProfilerQuery {
        let is_for_pass_timestamp_writes = true;
        let mut query =
            self.begin_query_internal(label.into(), is_for_pass_timestamp_writes, encoder);
        if let Some(timer_query) = &mut query.timer_query_pair {
            timer_query.usage_state = QueryPairUsageState::ReservedForPassTimestampWrites;
        }
        query
    }

    /// Ends passed query.
    ///
    /// If the passed query was opened with [`GpuProfiler::begin_query`], the passed encoder or pass must be the same
    /// as when the query was opened.
    pub fn end_query<Recorder: ProfilerCommandRecorder>(
        &self,
        encoder_or_pass: &mut Recorder,
        mut query: GpuProfilerQuery,
    ) {
        if let Some(timer_query) = &mut query.timer_query_pair {
            match timer_query.usage_state {
                QueryPairUsageState::Reserved => {
                    unreachable!("Query pair has been reserved but isn't used for anything!")
                }
                QueryPairUsageState::ReservedForPassTimestampWrites => {
                    // No need to do a timestamp write, this is handled by wgpu.
                }
                QueryPairUsageState::OnlyStartWritten => {
                    encoder_or_pass.write_timestamp(
                        &timer_query.pool.query_set,
                        timer_query.start_query_idx + 1,
                    );
                    timer_query.usage_state = QueryPairUsageState::BothStartAndEndWritten;
                }
                QueryPairUsageState::BothStartAndEndWritten => {
                    unreachable!("Query pair has already been used!")
                }
            }
        }

        #[cfg(feature = "tracy")]
        if let Some(ref mut tracy_scope) = query.tracy_scope {
            tracy_scope.end_zone();
        }

        if query.has_debug_group {
            encoder_or_pass.pop_debug_group();
        }

        let send_result = self.active_frame.closed_query_sender.send(query);

        // The only way we can fail sending the query is if the receiver has been dropped.
        // Since it sits on `active_frame` as well, there's no way for this to happen!
        debug_assert!(send_result.is_ok());

        // Count queries even if we haven't processed this one, makes experiences more consistent
        // if there's a lack of support for some queries.
        self.num_open_queries.fetch_sub(1, Ordering::Release);
    }

    /// Puts query resolve commands in the encoder for all unresolved, pending queries of the active profiler frame.
    ///
    /// Note that you do *not* need to do this for every encoder, it is sufficient do do this once per frame as long
    /// as you submit the corresponding command buffer after all others that may have opened queries in the same frame.
    /// (It does not matter if the passed encoder itself has previously opened queries or not.)
    /// If you were to make this part of a command buffer that is enqueued before any other that has
    /// opened queries in the same profiling frame, no failure will occur but some timing results may be invalid.
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

            debug_assert!(query_pool.capacity >= num_used_queries);
            debug_assert!(num_resolved_queries < num_used_queries);

            // Resolve into offset 0 of the resolve buffer - this way we don't have to worry about
            // the offset restrictions on resolve buffers (`wgpu::QUERY_RESOLVE_BUFFER_ALIGNMENT`)
            // and we copy it anyways.
            encoder.resolve_query_set(
                &query_pool.query_set,
                num_resolved_queries..num_used_queries,
                &query_pool.resolve_buffer,
                0,
            );
            // Copy the newly resolved queries into the read buffer, making sure
            // that we don't override any of the results that are already there.
            let destination_offset = (num_resolved_queries * wgpu::QUERY_SIZE) as u64;
            let copy_size = ((num_used_queries - num_resolved_queries) * wgpu::QUERY_SIZE) as u64;
            encoder.copy_buffer_to_buffer(
                &query_pool.resolve_buffer,
                0,
                &query_pool.read_buffer,
                destination_offset,
                copy_size,
            );

            query_pool
                .num_resolved_queries
                .store(num_used_queries, Ordering::Release);
        }
    }

    /// Marks the end of a frame.
    ///
    /// Needs to be called **after** submitting any encoder used in the current profiler frame.
    ///
    /// Fails if there are still open queries or unresolved queries.
    pub fn end_frame(&mut self) -> Result<(), EndFrameError> {
        let num_open_queries = self.num_open_queries.load(Ordering::Acquire);
        if num_open_queries != 0 {
            return Err(EndFrameError::UnclosedQueries(num_open_queries));
        }

        let query_pools = self.active_frame.query_pools.get_mut();

        let mut new_pending_frame = PendingFrame {
            query_pools: std::mem::take(&mut query_pools.used_pools),
            closed_query_by_parent_handle: HashMap::new(),
            mapped_buffers: Arc::new(AtomicU32::new(0)),
        };

        for query in self.active_frame.closed_query_receiver.get_mut().try_iter() {
            new_pending_frame
                .closed_query_by_parent_handle
                .entry(query.parent_handle)
                .or_default()
                .push(query);
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
                // Drop queries first since they still have references to the query pools that we want to reuse.
                drop(dropped_frame.closed_query_by_parent_handle);

                // Mark the frame as dropped. We'll give back the query pools once the mapping is done.
                // Any previously issued map_async call that haven't finished yet, will invoke their callback with mapping abort.
                self.reset_and_cache_unused_query_pools(dropped_frame.query_pools);
            }
        }

        // Map all buffers.
        for pool in new_pending_frame.query_pools.iter_mut() {
            let mapped_buffers = new_pending_frame.mapped_buffers.clone();
            pool.read_buffer
                .slice(0..(pool.num_used_queries.load(Ordering::Relaxed) * wgpu::QUERY_SIZE) as u64)
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
    /// `timestamp_period`:
    ///    The timestamp period of the device. Pass the result of [`wgpu::Queue::get_timestamp_period()`].
    ///    Note that some implementations (Chrome as of writing) may converge to a timestamp period while the application is running,
    ///    so caching this value is usually not recommended.
    pub fn process_finished_frame(
        &mut self,
        timestamp_period: f32,
    ) -> Option<Vec<GpuTimerQueryResult>> {
        let frame = self.pending_frames.first_mut()?;

        // We only process if all mappings succeed.
        if frame
            .mapped_buffers
            .load(std::sync::atomic::Ordering::Acquire)
            != frame.query_pools.len() as u32
        {
            return None;
        }

        let PendingFrame {
            query_pools,
            mut closed_query_by_parent_handle,
            mapped_buffers: _,
        } = self.pending_frames.remove(0);

        let results = {
            let timestamp_to_sec = timestamp_period as f64 / 1000.0 / 1000.0 / 1000.0;

            Self::process_timings_recursive(
                timestamp_to_sec,
                &mut closed_query_by_parent_handle,
                ROOT_QUERY_HANDLE,
            )
        };

        // Ensure that closed queries no longer hold references to the query pools.
        // `process_timings_recursive` should have handled this already.
        debug_assert!(closed_query_by_parent_handle.is_empty());
        drop(closed_query_by_parent_handle); // But just in case, we make sure to drop it here even if above debug assertion fails.

        self.reset_and_cache_unused_query_pools(query_pools);

        Some(results)
    }
}

// --------------------------------------------------------------------------------
// Internals
// --------------------------------------------------------------------------------

const QUERY_SET_MAX_QUERIES: u32 = wgpu::QUERY_SET_MAX_QUERIES;

/// Returns true if a timestamp query is supported.
fn timestamp_query_support<Recorder: ProfilerCommandRecorder>(
    is_for_pass_timestamp_writes: bool,
    encoder_or_pass: &mut Recorder,
    features: wgpu::Features,
) -> bool {
    let required_feature = if is_for_pass_timestamp_writes {
        wgpu::Features::TIMESTAMP_QUERY
    } else if encoder_or_pass.is_pass() {
        wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES
    } else {
        wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS
    };
    features.contains(required_feature)
}

impl GpuProfiler {
    fn next_scope_tree_handle(&self) -> GpuTimerQueryTreeHandle {
        // Relaxed is fine, we just want a number that nobody uses this frame already.
        let mut handle = self.next_query_handle.fetch_add(1, Ordering::Relaxed);

        // We don't ever expect to run out of handles during a single frame, but who knows how long the app runs.
        while handle == ROOT_QUERY_HANDLE {
            handle = self.next_query_handle.fetch_add(1, Ordering::Relaxed);
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
            // If timer queries were disabled, we also don't keep any pools.
            if self.settings.enable_timer_queries && pool.capacity >= capacity_threshold {
                self.active_frame
                    .query_pools
                    .get_mut()
                    .unused_pools
                    .push(pool);
            }
        }
    }

    fn try_reserve_query_pair(pool: &Arc<QueryPool>) -> Option<ReservedTimerQueryPair> {
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
                    return Some(ReservedTimerQueryPair {
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
    fn reserve_query_pair(&self) -> ReservedTimerQueryPair {
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
                    &self.device,
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
    fn begin_query_internal<Recorder: ProfilerCommandRecorder>(
        &self,
        label: String,
        is_for_pass_timestamp_writes: bool,
        encoder_or_pass: &mut Recorder,
    ) -> GpuProfilerQuery {
        // Give opening/closing queries acquire/release semantics:
        // This way, we won't get any nasty surprises when observing zero open queries.
        self.num_open_queries.fetch_add(1, Ordering::Acquire);

        let query = if self.settings.enable_timer_queries
            && timestamp_query_support(
                is_for_pass_timestamp_writes,
                encoder_or_pass,
                self.device.features(),
            ) {
            Some(self.reserve_query_pair())
        } else {
            None
        };

        let _tracy_scope = if self.settings.enable_timer_queries {
            #[cfg(feature = "tracy")]
            {
                let location = std::panic::Location::caller();
                self.tracy_context.as_ref().and_then(|c| {
                    c.span_alloc(&label, "", location.file(), location.line())
                        .ok()
                })
            }
            #[cfg(not(feature = "tracy"))]
            Option::<()>::None
        } else {
            None
        };

        let pid = if cfg!(target_arch = "wasm32") {
            0
        } else {
            std::process::id()
        };

        GpuProfilerQuery {
            label,
            pid,
            tid: std::thread::current().id(),
            timer_query_pair: query,
            handle: self.next_scope_tree_handle(),
            parent_handle: ROOT_QUERY_HANDLE,
            has_debug_group: false,
            #[cfg(feature = "tracy")]
            tracy_scope: _tracy_scope,
        }
    }

    fn process_timings_recursive(
        timestamp_to_sec: f64,
        closed_scope_by_parent_handle: &mut HashMap<GpuTimerQueryTreeHandle, Vec<GpuProfilerQuery>>,
        parent_handle: GpuTimerQueryTreeHandle,
    ) -> Vec<GpuTimerQueryResult> {
        let Some(queries_with_same_parent) = closed_scope_by_parent_handle.remove(&parent_handle)
        else {
            return Vec::new();
        };

        queries_with_same_parent
            .into_iter()
            .map(|mut scope| {
                // Note that inactive queries may still have nested queries, it's therefore important we process all of them.
                // In particular, this happens if only `wgpu::Features::TIMESTAMP_QUERY`` is enabled and `timestamp_writes`
                // on passes are nested inside inactive encoder timer queries.
                let time_raw = scope.timer_query_pair.take().map(|query| {
                    // Read timestamp from buffer.
                    // By design timestamps for start/end are consecutive.
                    let offset = (query.start_query_idx * wgpu::QUERY_SIZE) as u64;
                    let buffer_slice = &query
                        .pool
                        .read_buffer
                        .slice(offset..(offset + (wgpu::QUERY_SIZE * 2) as u64))
                        .get_mapped_range();
                    let start_raw = u64::from_le_bytes(
                        buffer_slice[0..wgpu::QUERY_SIZE as usize]
                            .try_into()
                            .unwrap(),
                    );
                    let end_raw = u64::from_le_bytes(
                        buffer_slice[wgpu::QUERY_SIZE as usize..(wgpu::QUERY_SIZE as usize) * 2]
                            .try_into()
                            .unwrap(),
                    );

                    start_raw..end_raw
                });

                let time = time_raw.as_ref().map(|time_raw| {
                    (time_raw.start as f64 * timestamp_to_sec)
                        ..(time_raw.end as f64 * timestamp_to_sec)
                });

                #[cfg(feature = "tracy")]
                if let (Some(tracy_scope), Some(time_raw)) = (&scope.tracy_scope, &time_raw) {
                    tracy_scope.upload_timestamp_start(time_raw.start as i64);
                }

                let nested_queries = Self::process_timings_recursive(
                    timestamp_to_sec,
                    closed_scope_by_parent_handle,
                    scope.handle,
                );

                #[cfg(feature = "tracy")]
                if let (Some(tracy_scope), Some(time_raw)) = (&scope.tracy_scope, time_raw) {
                    tracy_scope.upload_timestamp_end(time_raw.end as i64);
                }

                GpuTimerQueryResult {
                    label: std::mem::take(&mut scope.label),
                    time,
                    nested_queries,
                    pid: scope.pid,
                    tid: scope.tid,
                }
            })
            .collect::<Vec<_>>()
    }
}

#[derive(PartialEq, Eq)]
pub enum QueryPairUsageState {
    /// Transitional state used upon creation.
    Reserved,

    /// Don't do manual timestamp writes, wgpu is expected to do them for us.
    ReservedForPassTimestampWrites,

    /// Start query has been used, end query is still available.
    OnlyStartWritten,

    /// Both start & end query have been used.
    BothStartAndEndWritten,
}

pub struct ReservedTimerQueryPair {
    /// [`QueryPool`] on which both start & end queries of the scope are done.
    ///
    /// By putting an arc here instead of an index into a vec, we don't need
    /// need to take any locks upon closing a profiling scope.
    pub pool: Arc<QueryPool>,

    /// Query index at which the scope begins.
    /// The query after this is reserved for the end of the scope.
    pub start_query_idx: u32,

    /// Current use of the query pair.
    pub usage_state: QueryPairUsageState,
}

/// A pool of queries, consisting of a single queryset & buffer for query results.
#[derive(Debug)]
pub struct QueryPool {
    pub query_set: wgpu::QuerySet,

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
                size: (wgpu::QUERY_SIZE * capacity) as u64,
                usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            }),

            read_buffer: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("GpuProfiler - Query Read Buffer"),
                size: (wgpu::QUERY_SIZE * capacity) as u64,
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
    /// The last pool is the one new profiling queries will try to make timer queries into.
    used_pools: Vec<Arc<QueryPool>>,

    /// List of unused pools recycled from previous frames.
    unused_pools: Vec<QueryPool>,
}

/// Internal handle to building a tree of profiling queries.
pub type GpuTimerQueryTreeHandle = u32;

/// Handle for the root scope.
pub const ROOT_QUERY_HANDLE: GpuTimerQueryTreeHandle = u32::MAX;

struct ActiveFrame {
    query_pools: RwLock<PendingFramePools>,

    /// Closed queries get send to this channel.
    ///
    /// Note that channel is still overkill for what we want here:
    /// We're in a multi producer situation, *but* the single consumer is known to be only
    /// active in a mut context, i.e. while we're consuming we know that we're not producing.
    /// We have to wrap it in a Mutex because the channel is not Sync, but we actually never lock it
    /// since we only ever access it in a `mut` context.
    closed_query_sender: std::sync::mpsc::Sender<GpuProfilerQuery>,
    closed_query_receiver: Mutex<std::sync::mpsc::Receiver<GpuProfilerQuery>>,
}

struct PendingFrame {
    query_pools: Vec<Arc<QueryPool>>,
    closed_query_by_parent_handle: HashMap<GpuTimerQueryTreeHandle, Vec<GpuProfilerQuery>>,

    /// Keeps track of the number of buffers in the query pool that have been mapped successfully.
    mapped_buffers: std::sync::Arc<std::sync::atomic::AtomicU32>,
}
