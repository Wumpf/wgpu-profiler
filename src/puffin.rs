use puffin::{GlobalProfiler, NanoSecond, ScopeDetails, StreamInfo, ThreadInfo};

use crate::GpuTimerQueryResult;

/// Visualize the query results in a `puffin::GlobalProfiler`.
pub fn output_frame_to_puffin(profiler: &mut GlobalProfiler, query_result: &[GpuTimerQueryResult]) {
    let mut stream_info = StreamInfo::default();
    collect_stream_info_recursive(profiler, &mut stream_info, query_result, 0);

    profiler.report_user_scopes(
        ThreadInfo {
            start_time_ns: None,
            name: "GPU".to_string(),
        },
        &stream_info.as_stream_into_ref(),
    );
}

fn collect_stream_info_recursive(
    profiler: &mut GlobalProfiler,
    stream_info: &mut StreamInfo,
    query_result: &[GpuTimerQueryResult],
    depth: usize,
) {
    let details: Vec<_> = query_result
        .iter()
        .map(|query| ScopeDetails::from_scope_name(query.label.clone()))
        .collect();
    let ids = profiler.register_user_scopes(&details);
    for (query, id) in query_result.iter().zip(ids) {
        if let Some(time) = &query.time {
            let start = (time.start * 1e9) as NanoSecond;
            let end = (time.end * 1e9) as NanoSecond;

            stream_info.depth = stream_info.depth.max(depth);
            stream_info.num_scopes += 1;
            stream_info.range_ns.0 = stream_info.range_ns.0.min(start);
            stream_info.range_ns.1 = stream_info.range_ns.0.max(end);

            let (offset, _) = stream_info.stream.begin_scope(|| start, id, "");
            collect_stream_info_recursive(profiler, stream_info, &query.nested_queries, depth + 1);
            stream_info.stream.end_scope(offset, end as NanoSecond);
        }
    }
}
