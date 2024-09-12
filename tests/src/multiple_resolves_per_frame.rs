// Regression test for bug described in https://github.com/Wumpf/wgpu-profiler/issues/79
#[test]
fn multiple_resolves_per_frame() {
    let (_, device, queue) = super::create_device(
        wgpu::Features::TIMESTAMP_QUERY.union(wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS),
    )
    .unwrap();

    let mut profiler =
        wgpu_profiler::GpuProfiler::new(wgpu_profiler::GpuProfilerSettings::default()).unwrap();

    {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

        // Resolve call per scope.
        {
            let _ = profiler.scope("testscope0", &mut encoder, &device);
        }
        profiler.resolve_queries(&mut encoder);
        {
            let _ = profiler.scope("testscope1", &mut encoder, &device);
        }
        profiler.resolve_queries(&mut encoder);

        // And an extra resolve for good measure (this should be a no-op).
        profiler.resolve_queries(&mut encoder);

        profiler.end_frame().unwrap();
    }

    // Poll to explicitly trigger mapping callbacks.
    device.poll(wgpu::Maintain::Wait);

    // Frame should now be available.
    assert!(profiler
        .process_finished_frame(queue.get_timestamp_period())
        .is_some());
}
