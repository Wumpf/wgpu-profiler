use std::{convert::TryInto, ops::Range};

pub mod chrometrace;
pub mod macros;
/// Scope types that wrap a `wgpu` encoder/pass and start a scope on creation. In most cases, they
/// then allow automatically ending the scope on drop.
pub mod scope;

pub struct GpuTimerScopeResult {
    pub label: String,
    /// Time range of this scope in seconds.
    /// Meaning of absolute value is not defined.
    pub time: Range<f64>,

    pub nested_scopes: Vec<GpuTimerScopeResult>,
}

pub struct GpuProfiler {
    enable_pass_timer: bool,
    enable_encoder_timer: bool,
    pub enable_debug_marker: bool,

    unused_pools: Vec<QueryPool>,

    pending_frames: Vec<PendingFrame>,
    active_frame: PendingFrame,
    open_scopes: Vec<UnprocessedTimerScope>,

    size_for_new_query_pools: u32,

    max_num_pending_frames: usize,
    timestamp_to_sec: f64,
}

// Public interface
#[deny(missing_docs)]
impl GpuProfiler {
    /// Required wgpu features for timer scopes.
    pub const REQUIRED_WGPU_FEATURES: wgpu::Features = wgpu::Features::TIMESTAMP_QUERY.union(wgpu::Features::WRITE_TIMESTAMP_INSIDE_PASSES);

    /// Creates a new Profiler object.
    ///
    /// There is nothing preventing the use of several independent profiler objects.
    ///
    /// `active_features` should contain the features enabled on the device to
    /// be used in the profiler scopes, these will be used to determine what
    /// queries are supported and configure the profiler accordingly
    /// (see [`GpuProfiler::REQUIRED_WGPU_FEATURES`])
    ///
    /// A profiler queues up to `max_num_pending_frames` "profiler-frames" at a time.
    /// A profiler-frame is in-flight until its queries have been successfully resolved using [`GpuProfiler::process_finished_frame`].
    /// If this threshold is reached, [`GpuProfiler::end_frame`] will drop frames.
    /// (Typical values for `max_num_pending_frames` are 2~4)
    ///
    /// `timestamp_period` needs to be set to the result of [`wgpu::Queue::get_timestamp_period`]
    pub fn new(max_num_pending_frames: usize, timestamp_period: f32, active_features: wgpu::Features) -> Self {
        assert!(max_num_pending_frames > 0);
        GpuProfiler {
            enable_pass_timer: active_features.contains(wgpu::Features::TIMESTAMP_QUERY),
            enable_encoder_timer: active_features.contains(wgpu::Features::WRITE_TIMESTAMP_INSIDE_PASSES),
            enable_debug_marker: true,

            unused_pools: Vec::new(),

            pending_frames: Vec::new(),
            active_frame: PendingFrame {
                query_pools: Vec::new(),
                mapped_buffers: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
                closed_scopes: Vec::new(),
            },
            open_scopes: Vec::new(),

            size_for_new_query_pools: QueryPool::MIN_CAPACITY,

            max_num_pending_frames,
            timestamp_to_sec: timestamp_period as f64 / 1000.0 / 1000.0 / 1000.0,
        }
    }

    /// Starts a new debug/timer scope on a given encoder or rendering/compute pass.
    ///
    /// Scopes can be arbitrarily nested.
    ///
    /// May create new wgpu query objects (which is why it needs a [`wgpu::Device`] reference)
    ///
    /// See also [`wgpu_profiler!`], [`GpuProfiler::end_scope`]
    pub fn begin_scope<Recorder: ProfilerCommandRecorder>(&mut self, label: &str, encoder_or_pass: &mut Recorder, device: &wgpu::Device) {
        if (encoder_or_pass.is_pass() && self.enable_pass_timer) || (!encoder_or_pass.is_pass() && self.enable_encoder_timer) {
            let start_query = self.allocate_query_pair(device);

            encoder_or_pass.write_timestamp(
                &self.active_frame.query_pools[start_query.pool_idx as usize].query_set,
                start_query.query_idx,
            );

            self.open_scopes.push(UnprocessedTimerScope {
                label: String::from(label),
                start_query,
                ..Default::default()
            });
        }
        if self.enable_debug_marker {
            encoder_or_pass.push_debug_group(label);
        }
    }

    /// Ends a debug/timer scope.
    ///
    /// Panics if no scope has been open previously.
    ///
    /// See also [`wgpu_profiler!`], [`GpuProfiler::begin_scope`]
    pub fn end_scope<Recorder: ProfilerCommandRecorder>(&mut self, encoder_or_pass: &mut Recorder) {
        if (encoder_or_pass.is_pass() && self.enable_pass_timer) || (!encoder_or_pass.is_pass() && self.enable_encoder_timer) {
            let open_scope = self.open_scopes.pop().expect("No profiler GpuProfiler scope was previously opened");
            encoder_or_pass.write_timestamp(
                &self.active_frame.query_pools[open_scope.start_query.pool_idx as usize].query_set,
                open_scope.start_query.query_idx + 1,
            );
            if let Some(open_parent_scope) = self.open_scopes.last_mut() {
                open_parent_scope.nested_scopes.push(open_scope);
            } else {
                self.active_frame.closed_scopes.push(open_scope);
            }
        }
        if self.enable_debug_marker {
            encoder_or_pass.pop_debug_group();
        }
    }

    /// Puts query resolve commands in the encoder for all unresolved, pending queries of the current profiler frame.
    pub fn resolve_queries(&mut self, encoder: &mut wgpu::CommandEncoder) {
        for query_pool in self.active_frame.query_pools.iter_mut() {
            if query_pool.num_resolved_queries == query_pool.num_used_queries {
                continue;
            }
            assert!(query_pool.num_resolved_queries < query_pool.num_used_queries);
            encoder.resolve_query_set(
                &query_pool.query_set,
                query_pool.num_resolved_queries..query_pool.num_used_queries,
                &query_pool.buffer,
                (query_pool.num_resolved_queries * QUERY_SIZE) as u64,
            );
            query_pool.num_resolved_queries = query_pool.num_used_queries;
        }
    }

    /// Marks the end of a frame.
    /// Needs to be called AFTER submitting any encoder used in the current frame.
    #[allow(clippy::result_unit_err)]
    pub fn end_frame(&mut self) -> Result<(), ()> {
        // TODO: Error messages
        if !self.open_scopes.is_empty() {
            return Err(());
        }
        if self
            .active_frame
            .query_pools
            .iter()
            .any(|pool| pool.num_resolved_queries != pool.num_used_queries)
        {
            return Err(());
        }

        self.size_for_new_query_pools = self
            .size_for_new_query_pools
            .max(self.active_frame.query_pools.iter().map(|pool| pool.num_used_queries).sum())
            .min(QUERY_SET_MAX_QUERIES);

        // Make sure we don't overflow
        if self.pending_frames.len() == self.max_num_pending_frames {
            // Drop previous frame.
            let dropped_frame = self.pending_frames.pop().unwrap();
            self.cache_unused_query_pools(dropped_frame.query_pools);
            // TODO report this somehow
        }

        // Map all buffers.
        for pool in self.active_frame.query_pools.iter_mut() {
            let mapped_buffers = self.active_frame.mapped_buffers.clone();
            pool.resolved_buffer_slice().map_async(wgpu::MapMode::Read, move |res| {
                res.unwrap();
                mapped_buffers.fetch_add(1, std::sync::atomic::Ordering::Release);
            });
        }

        // Enqueue
        let mut frame = Default::default();
        std::mem::swap(&mut frame, &mut self.active_frame);
        self.pending_frames.push(frame);

        assert!(self.pending_frames.len() <= self.max_num_pending_frames);

        Ok(())
    }

    /// Checks if all timer queries for the oldest pending finished frame are done and returns that snapshot if any.
    pub fn process_finished_frame(&mut self) -> Option<Vec<GpuTimerScopeResult>> {
        let frame = self.pending_frames.first_mut()?;

        // We only process if all mappings succeed.
        if frame.mapped_buffers.load(std::sync::atomic::Ordering::Acquire) != frame.query_pools.len() {
            return None;
        }

        let frame = self.pending_frames.remove(0);

        let results = {
            let resolved_query_buffers: Vec<wgpu::BufferView> = frame
                .query_pools
                .iter()
                .map(|pool| pool.resolved_buffer_slice().get_mapped_range())
                .collect();
            Self::process_timings_recursive(self.timestamp_to_sec, &resolved_query_buffers, frame.closed_scopes)
        };

        self.cache_unused_query_pools(frame.query_pools);

        Some(results)
    }
}

// --------------------------------------------------------------------------------
// Internals
// --------------------------------------------------------------------------------

const QUERY_SIZE: u32 = 8; // Newer wgpu version have QUERY_SIZE
const QUERY_SET_MAX_QUERIES: u32 = 8192; // Newer wgpu version have QUERY_SET_MAX_QUERIES

impl GpuProfiler {
    fn cache_unused_query_pools(&mut self, mut query_pools: Vec<QueryPool>) {
        // If a pool was less than half of the size of the max frame, then we don't keep it.
        // This way we're going to need less pools in upcoming frames and thus have less overhead in the long run.
        let capacity_threshold = self.size_for_new_query_pools / 2;
        for mut pool in query_pools.drain(..) {
            pool.reset();
            if pool.capacity >= capacity_threshold {
                self.unused_pools.push(pool);
            }
        }
    }

    // Reserves two query objects.
    // Our query pools always have an even number of queries, so we know the next query is the next in the same pool.
    fn allocate_query_pair(&mut self, device: &wgpu::Device) -> QueryPoolQueryAddress {
        let num_pools = self.active_frame.query_pools.len();

        if let Some(active_pool) = self.active_frame.query_pools.last_mut() {
            if active_pool.capacity > active_pool.num_used_queries {
                let address = QueryPoolQueryAddress {
                    pool_idx: num_pools as u32 - 1,
                    query_idx: active_pool.num_used_queries,
                };
                active_pool.num_used_queries += 2;
                assert!(active_pool.num_used_queries <= active_pool.capacity);
                return address;
            }
        }

        let mut new_pool = if let Some(reused_pool) = self.unused_pools.pop() {
            reused_pool
        } else {
            QueryPool::new(
                self.active_frame
                    .query_pools
                    .iter()
                    .map(|pool| pool.capacity)
                    .sum::<u32>()
                    .max(self.size_for_new_query_pools)
                    .min(QUERY_SET_MAX_QUERIES),
                device,
            )
        };
        new_pool.num_used_queries += 2;
        self.active_frame.query_pools.push(new_pool);

        QueryPoolQueryAddress {
            pool_idx: self.active_frame.query_pools.len() as u32 - 1,
            query_idx: 0,
        }
    }

    fn process_timings_recursive(
        timestamp_to_sec: f64,
        resolved_query_buffers: &[wgpu::BufferView],
        unprocessed_scopes: Vec<UnprocessedTimerScope>,
    ) -> Vec<GpuTimerScopeResult> {
        unprocessed_scopes
            .into_iter()
            .map(|scope| {
                let nested_scopes = if scope.nested_scopes.is_empty() {
                    Vec::new()
                } else {
                    Self::process_timings_recursive(timestamp_to_sec, resolved_query_buffers, scope.nested_scopes)
                };

                // By design timestamps for start/end are consecutive.
                let buffer = &resolved_query_buffers[scope.start_query.pool_idx as usize];
                let offset = (scope.start_query.query_idx * QUERY_SIZE) as usize;
                let start_raw = u64::from_le_bytes(buffer[offset..(offset + std::mem::size_of::<u64>())].try_into().unwrap());
                let end_raw = u64::from_le_bytes(
                    buffer[(offset + std::mem::size_of::<u64>())..(offset + std::mem::size_of::<u64>() * 2)]
                        .try_into()
                        .unwrap(),
                );

                GpuTimerScopeResult {
                    label: scope.label,
                    time: (start_raw as f64 * timestamp_to_sec)..(end_raw as f64 * timestamp_to_sec),
                    nested_scopes,
                }
            })
            .collect()
    }
}

#[derive(Default)]
struct QueryPoolQueryAddress {
    pool_idx: u32,
    query_idx: u32,
}

#[derive(Default)]
struct UnprocessedTimerScope {
    label: String,
    start_query: QueryPoolQueryAddress,
    nested_scopes: Vec<UnprocessedTimerScope>,
}

struct QueryPool {
    query_set: wgpu::QuerySet,

    buffer: wgpu::Buffer,

    capacity: u32,
    num_used_queries: u32,
    num_resolved_queries: u32,
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

            buffer: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("GpuProfiler - Query Buffer"),
                size: (QUERY_SIZE * capacity) as u64,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            }),

            capacity,
            num_used_queries: 0,
            num_resolved_queries: 0,
        }
    }

    fn reset(&mut self) {
        self.num_used_queries = 0;
        self.num_resolved_queries = 0;
        self.buffer.unmap();
    }

    fn resolved_buffer_slice(&self) -> wgpu::BufferSlice {
        self.buffer.slice(0..(self.num_resolved_queries * QUERY_SIZE) as u64)
    }
}

#[derive(Default)]
struct PendingFrame {
    query_pools: Vec<QueryPool>,
    mapped_buffers: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    closed_scopes: Vec<UnprocessedTimerScope>,
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
