use std::{ops::Range, thread::ThreadId};

use crate::profiler::{
    GpuTimerQueryTreeHandle, QueryPairUsageState, ReservedTimerQueryPair, ROOT_QUERY_HANDLE,
};

/// The result of a gpu timer scope.
#[derive(Debug, Clone)]
pub struct GpuTimerQueryResult {
    /// Label that was specified when opening the scope.
    pub label: String,

    /// The process id of the process that opened this scope.
    pub pid: u32,

    /// The thread id of the thread that opened this scope.
    pub tid: ThreadId,

    /// Time range of this scope in seconds.
    ///
    /// Meaning of absolute value is not defined.
    /// If timestamp writing was disabled for this scope, this is None.
    pub time: Option<Range<f64>>,

    /// Scopes that were opened while this scope was open.
    pub nested_queries: Vec<GpuTimerQueryResult>,
}

/// An inflight query for the profiler.
///
/// If timer queries are enabled, this represents a reserved timer query pair on
/// one of the profiler's query sets.
/// *Must* be closed by calling [`GpuProfiler::end_query`].
///
/// Emitted by [`GpuProfiler::begin_query`]/[`GpuProfiler::begin_pass_query`] and consumed by [`GpuProfiler::end_query`].
///
/// [`GpuProfiler::begin_pass_query`]: crate::GpuProfiler::begin_pass_query
/// [`GpuProfiler::begin_query`]: crate::GpuProfiler::begin_query
/// [`GpuProfiler::end_query`]: crate::GpuProfiler::end_query
pub struct GpuProfilerQuery {
    /// The label assigned to this query.
    /// Will be moved into [`GpuProfilerQuery::label`] once the query is fully processed.
    pub label: String,

    /// The process id of the process that opened this query.
    pub pid: u32,

    /// The thread id of the thread that opened this query.
    pub tid: ThreadId,

    /// The actual query on a query pool if any (none if disabled for this type of query).
    pub(crate) timer_query_pair: Option<ReservedTimerQueryPair>,

    /// Handle which identifies this query, used for building the tree of queries.
    pub(crate) handle: GpuTimerQueryTreeHandle,

    /// Which query this query is a child of.
    pub(crate) parent_handle: GpuTimerQueryTreeHandle,

    /// Whether a debug group was opened for this scope.
    pub(crate) has_debug_group: bool,

    #[cfg(feature = "tracy")]
    pub(crate) tracy_scope: Option<tracy_client::GpuSpan>,
}

impl GpuProfilerQuery {
    /// Use the reserved query for render pass timestamp writes if any.
    ///
    /// Use this only for a single render/compute pass, otherwise results will be overwritten.
    /// Only ever returns `Some` for queries that were created using [`GpuProfiler::begin_pass_query`].
    ///
    /// [`GpuProfiler::begin_pass_query`]: crate::GpuProfiler::begin_pass_query
    pub fn render_pass_timestamp_writes(&self) -> Option<wgpu::RenderPassTimestampWrites<'_>> {
        self.timer_query_pair.as_ref().and_then(|query| {
            (query.usage_state == QueryPairUsageState::ReservedForPassTimestampWrites).then(|| {
                wgpu::RenderPassTimestampWrites {
                    query_set: &query.pool.query_set,
                    beginning_of_pass_write_index: Some(query.start_query_idx),
                    end_of_pass_write_index: Some(query.start_query_idx + 1),
                }
            })
        })
    }

    /// Use the reserved query for compute pass timestamp writes if any.
    ///
    /// Use this only for a single render/compute pass, otherwise results will be overwritten.
    /// Only ever returns `Some` for queries that were created using [`GpuProfiler::begin_pass_query`].
    ///
    /// [`GpuProfiler::begin_pass_query`]: crate::GpuProfiler::begin_pass_query
    pub fn compute_pass_timestamp_writes(&self) -> Option<wgpu::ComputePassTimestampWrites<'_>> {
        self.timer_query_pair.as_ref().and_then(|query| {
            (query.usage_state == QueryPairUsageState::ReservedForPassTimestampWrites).then(|| {
                wgpu::ComputePassTimestampWrites {
                    query_set: &query.pool.query_set,
                    beginning_of_pass_write_index: Some(query.start_query_idx),
                    end_of_pass_write_index: Some(query.start_query_idx + 1),
                }
            })
        })
    }

    /// Makes this scope a child of the passed scope.
    #[inline]
    pub fn with_parent(self, parent: Option<&GpuProfilerQuery>) -> Self {
        Self {
            parent_handle: parent.map_or(ROOT_QUERY_HANDLE, |p| p.handle),
            ..self
        }
    }
}
