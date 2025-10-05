// Regression test for bug described in
// * https://github.com/Wumpf/wgpu-profiler/issues/79
// * https://github.com/Wumpf/wgpu-profiler/issues/82
#[test]
fn multiple_resolves_per_frame() {
    const NUM_SCOPES: usize = 1000;

    let (_, device, queue) = super::create_device(
        wgpu::Features::TIMESTAMP_QUERY.union(wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS),
    )
    .unwrap();

    let mut profiler =
        wgpu_profiler::GpuProfiler::new(&device, wgpu_profiler::GpuProfilerSettings::default())
            .unwrap();

    {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

        // Resolve call per scope.
        // Do this many times to check for potential buffer overflows as found in
        // https://github.com/Wumpf/wgpu-profiler/issues/82
        for i in 0..NUM_SCOPES {
            {
                let _ = profiler.scope(format!("{i}"), &mut encoder);
            }
            profiler.resolve_queries(&mut encoder);
        }

        // And an extra resolve for good measure (this should be a no-op).
        profiler.resolve_queries(&mut encoder);

        profiler.end_frame().unwrap();
    }

    // Poll to explicitly trigger mapping callbacks.
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();

    // Frame should now be available and contain all the scopes.
    let scopes = profiler
        .process_finished_frame(queue.get_timestamp_period())
        .unwrap();
    assert_eq!(scopes.len(), NUM_SCOPES);
    for (i, scope) in scopes.iter().enumerate() {
        assert_eq!(scope.label, format!("{i}"));
    }
}
