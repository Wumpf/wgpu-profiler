use wgpu_profiler::GpuProfilerSettings;

use super::create_device;

// regression test for bug described in https://github.com/Wumpf/wgpu-profiler/pull/18
#[test]
fn handle_dropped_frames_gracefully() {
    let (_, device, queue) = create_device(
        wgpu::Features::TIMESTAMP_QUERY.union(wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS),
    )
    .unwrap();

    // max_num_pending_frames is one!
    let mut profiler = wgpu_profiler::GpuProfiler::new(
        &device,
        GpuProfilerSettings {
            max_num_pending_frames: 1,
            ..Default::default()
        },
    )
    .unwrap();

    // Two frames without device poll, causing the profiler to drop a frame on the second round.
    for _ in 0..2 {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let _ = profiler.scope("testscope", &mut encoder);
        }
        profiler.resolve_queries(&mut encoder);
        profiler.end_frame().unwrap();

        // We haven't done a device poll, so there can't be a result!
        assert!(profiler
            .process_finished_frame(queue.get_timestamp_period())
            .is_none());
    }

    // Poll to explicitly trigger mapping callbacks.
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();

    // A single (!) frame should now be available.
    assert!(profiler
        .process_finished_frame(queue.get_timestamp_period())
        .is_some());
    assert!(profiler
        .process_finished_frame(queue.get_timestamp_period())
        .is_none());
}
