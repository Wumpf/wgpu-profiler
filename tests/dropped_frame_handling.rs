use wgpu_profiler::GpuProfilerSettings;

mod utils;

use utils::create_device;

// regression test for bug described in https://github.com/Wumpf/wgpu-profiler/pull/18
fn handle_dropped_frames_gracefully() {
    let (adapter, device, queue) = create_device(wgpu::Features::TIMESTAMP_QUERY);

    // max_num_pending_frames is one!
    let mut profiler = wgpu_profiler::GpuProfiler::new(
        &adapter,
        &device,
        &queue,
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
            let _ = wgpu_profiler::scope::Scope::start("testscope", &mut profiler, &mut encoder, &device);
        }
        profiler.resolve_queries(&mut encoder);
        profiler.end_frame().unwrap();

        // We haven't done a device poll, so there can't be a result!
        assert!(profiler.process_finished_frame().is_none());
    }

    // Poll to explicitly trigger mapping callbacks.
    device.poll(wgpu::Maintain::Wait);

    // A single (!) frame should now be available.
    assert!(profiler.process_finished_frame().is_some());
    assert!(profiler.process_finished_frame().is_none());
}
