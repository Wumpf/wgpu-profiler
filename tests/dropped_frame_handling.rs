use wgpu_profiler::GpuProfilerSettings;

#[test]
fn handle_dropped_frames_gracefully() {
    futures_lite::future::block_on(handle_dropped_frames_gracefully_async());
}

// regression test for bug described in https://github.com/Wumpf/wgpu-profiler/pull/18
async fn handle_dropped_frames_gracefully_async() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
    let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions::default()).await.unwrap();
    let (device, queue) = adapter
        .request_device(
            &wgpu::DeviceDescriptor {
                features: wgpu::Features::TIMESTAMP_QUERY,
                ..Default::default()
            },
            None,
        )
        .await
        .unwrap();

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
