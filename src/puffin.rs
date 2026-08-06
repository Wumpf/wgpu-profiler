use std::collections::HashMap;

use puffin::{GlobalProfiler, NanoSecond, ScopeDetails, ScopeId, StreamInfo, ThreadInfo};

use crate::GpuTimerQueryResult;

/// Cache for registered puffin scope IDs to avoid memory leaks.
#[derive(Default)]
pub struct PuffinScopeCache {
    scope_ids: HashMap<String, ScopeId>,
}

/// Visualize the query results in a `puffin::GlobalProfiler`.
pub fn output_frame_to_puffin(
    profiler: &mut GlobalProfiler,
    cache: &mut PuffinScopeCache,
    query_result: &[GpuTimerQueryResult],
) {
    let mut stream_info = StreamInfo::default();
    build_stream_info(profiler, cache, &mut stream_info, query_result, 0);

    profiler.report_user_scopes(
        ThreadInfo {
            start_time_ns: None,
            name: "GPU".to_string(),
        },
        &stream_info.as_stream_into_ref(),
    );
}

fn build_stream_info(
    profiler: &mut GlobalProfiler,
    cache: &mut PuffinScopeCache,
    stream_info: &mut StreamInfo,
    query_result: &[GpuTimerQueryResult],
    depth: usize,
) {
    for query in query_result {
        // Use get() first to avoid cloning the label on cache hits.
        let id = if let Some(&id) = cache.scope_ids.get(&query.label) {
            id
        } else {
            let details = [ScopeDetails::from_scope_name(query.label.clone())];
            let id = profiler.register_user_scopes(&details)[0];
            cache.scope_ids.insert(query.label.clone(), id);
            id
        };

        if let Some(time) = &query.time {
            let start = (time.start * 1e9) as NanoSecond;
            let end = (time.end * 1e9) as NanoSecond;

            stream_info.depth = stream_info.depth.max(depth);
            stream_info.num_scopes += 1;
            stream_info.range_ns.0 = stream_info.range_ns.0.min(start);
            stream_info.range_ns.1 = stream_info.range_ns.1.max(end);

            let (offset, _) = stream_info.stream.begin_scope(|| start, id, "");
            build_stream_info(
                profiler,
                cache,
                stream_info,
                &query.nested_queries,
                depth + 1,
            );
            stream_info.stream.end_scope(offset, end as NanoSecond);
        }
    }
}
