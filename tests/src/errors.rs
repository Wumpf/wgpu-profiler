use wgpu_profiler::GpuProfilerSettings;

use super::create_device;

#[test]
fn invalid_pending_frame_count() {
    let profiler = wgpu_profiler::GpuProfiler::new(wgpu_profiler::GpuProfilerSettings {
        max_num_pending_frames: 0,
        ..Default::default()
    });
    assert!(matches!(
        profiler,
        Err(wgpu_profiler::CreationError::InvalidSettings(
            wgpu_profiler::SettingsError::InvalidMaxNumPendingFrames
        ))
    ));
}

#[test]
fn end_frame_unclosed_scope() {
    let (_, device, _) = create_device(wgpu::Features::TIMESTAMP_QUERY).unwrap();

    let mut profiler = wgpu_profiler::GpuProfiler::new(GpuProfilerSettings::default()).unwrap();
    let unclosed_scope = {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        let scope = profiler.begin_scope("open scope", &mut encoder, &device);
        profiler.resolve_queries(&mut encoder);
        scope
    };

    assert_eq!(
        profiler.end_frame(),
        Err(wgpu_profiler::EndFrameError::UnclosedScopes(1))
    );

    // Make sure we can recover from this.
    {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        profiler.end_scope(&mut encoder, unclosed_scope);
        profiler.resolve_queries(&mut encoder);
    }
    assert_eq!(profiler.end_frame(), Ok(()));
}

#[test]
fn end_frame_unresolved_scope() {
    let (_, device, _) = create_device(wgpu::Features::TIMESTAMP_QUERY).unwrap();

    let mut profiler = wgpu_profiler::GpuProfiler::new(GpuProfilerSettings::default()).unwrap();
    {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        let scope = profiler.begin_scope("open scope", &mut encoder, &device);
        profiler.end_scope(&mut encoder, scope);
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

    device.poll(wgpu::MaintainBase::Wait);
}

#[test]
fn change_settings_while_scope_open() {
    let (_, device, _) = create_device(wgpu::Features::TIMESTAMP_QUERY).unwrap();

    let mut profiler = wgpu_profiler::GpuProfiler::new(GpuProfilerSettings::default()).unwrap();

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    let scope = profiler.begin_scope("open scope", &mut encoder, &device);

    assert_eq!(
        profiler.change_settings(GpuProfilerSettings::default()),
        Err(wgpu_profiler::SettingsError::HasOpenScopes)
    );

    profiler.end_scope(&mut encoder, scope);
}
