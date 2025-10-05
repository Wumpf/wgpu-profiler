use wgpu_profiler::GpuProfilerSettings;

use super::create_device;

#[test]
fn invalid_pending_frame_count() {
    let (_, device, _queue) = create_device(wgpu::Features::TIMESTAMP_QUERY).unwrap();

    let profiler = wgpu_profiler::GpuProfiler::new(
        &device,
        wgpu_profiler::GpuProfilerSettings {
            max_num_pending_frames: 0,
            ..Default::default()
        },
    );
    assert!(matches!(
        profiler,
        Err(wgpu_profiler::CreationError::InvalidSettings(
            wgpu_profiler::SettingsError::InvalidMaxNumPendingFrames
        ))
    ));
}

#[test]
fn end_frame_unclosed_query() {
    let (_, device, _queue) = create_device(
        wgpu::Features::TIMESTAMP_QUERY.union(wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS),
    )
    .unwrap();

    let mut profiler =
        wgpu_profiler::GpuProfiler::new(&device, GpuProfilerSettings::default()).unwrap();
    let unclosed_query = {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        let query = profiler.begin_query("open query", &mut encoder);
        profiler.resolve_queries(&mut encoder);
        query
    };

    assert_eq!(
        profiler.end_frame(),
        Err(wgpu_profiler::EndFrameError::UnclosedQueries(1))
    );

    // Make sure we can recover from this.
    {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        profiler.end_query(&mut encoder, unclosed_query);
        profiler.resolve_queries(&mut encoder);
    }
    assert_eq!(profiler.end_frame(), Ok(()));
}

#[test]
fn end_frame_unresolved_query() {
    let (_, device, _queue) = create_device(
        wgpu::Features::TIMESTAMP_QUERY.union(wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS),
    )
    .unwrap();

    let mut profiler =
        wgpu_profiler::GpuProfiler::new(&device, GpuProfilerSettings::default()).unwrap();
    {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        let query = profiler.begin_query("open query", &mut encoder);
        profiler.end_query(&mut encoder, query);
    }

    assert_eq!(
        profiler.end_frame(),
        Err(wgpu_profiler::EndFrameError::UnresolvedQueries(2))
    );

    // Make sure we can recover from this!
    {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        profiler.resolve_queries(&mut encoder);
    }
    assert_eq!(profiler.end_frame(), Ok(()));

    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
}

#[test]
fn change_settings_while_query_open() {
    let (_, device, _queue) = create_device(
        wgpu::Features::TIMESTAMP_QUERY.union(wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS),
    )
    .unwrap();

    let mut profiler =
        wgpu_profiler::GpuProfiler::new(&device, GpuProfilerSettings::default()).unwrap();

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    let query = profiler.begin_query("open query", &mut encoder);

    assert_eq!(
        profiler.change_settings(GpuProfilerSettings::default()),
        Ok(())
    );

    profiler.end_query(&mut encoder, query);
}
