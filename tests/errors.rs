use wgpu_profiler::GpuProfilerSettings;

fn create_device() -> (wgpu::Adapter, wgpu::Device, wgpu::Queue) {
    async fn create_default_device_async() -> (wgpu::Adapter, wgpu::Device, wgpu::Queue) {
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
        (adapter, device, queue)
    }

    futures_lite::future::block_on(create_default_device_async())
}

#[test]
fn invalid_pending_frame_count() {
    let (adapter, device, queue) = create_device();

    let profiler = wgpu_profiler::GpuProfiler::new(
        &adapter,
        &device,
        &queue,
        wgpu_profiler::GpuProfilerSettings {
            max_num_pending_frames: 0,
            ..Default::default()
        },
    );
    assert!(matches!(profiler, Err(wgpu_profiler::CreationError::InvalidMaxNumPendingFrames)));
}

#[test]
fn end_frame_unclosed_scope() {
    let (adapter, device, queue) = create_device();

    let mut profiler = wgpu_profiler::GpuProfiler::new(&adapter, &device, &queue, GpuProfilerSettings::default()).unwrap();
    {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        profiler.begin_scope("open scope", &mut encoder, &device);
        profiler.resolve_queries(&mut encoder);
    }

    assert_eq!(
        profiler.end_frame(),
        Err(wgpu_profiler::EndFrameError::UnclosedScopes(vec!["open scope".to_string()]))
    );

    // Make sure we can recover from this.
    {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        profiler.end_scope(&mut encoder).unwrap();
        profiler.resolve_queries(&mut encoder);
    }
    assert_eq!(profiler.end_frame(), Ok(()));
}

#[test]
fn end_frame_unresolved_scope() {
    let (adapter, device, queue) = create_device();

    let mut profiler = wgpu_profiler::GpuProfiler::new(&adapter, &device, &queue, GpuProfilerSettings::default()).unwrap();
    {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        profiler.begin_scope("open scope", &mut encoder, &device);
        profiler.end_scope(&mut encoder).unwrap();
    }

    assert_eq!(profiler.end_frame(), Err(wgpu_profiler::EndFrameError::UnresolvedQueries(2)));

    // Make sure we can recover from this!
    {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        profiler.resolve_queries(&mut encoder);
    }
    assert_eq!(profiler.end_frame(), Ok(()));
}

#[test]
fn no_open_scope() {
    let (adapter, device, queue) = create_device();

    let mut profiler = wgpu_profiler::GpuProfiler::new(&adapter, &device, &queue, GpuProfilerSettings::default()).unwrap();
    {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        assert_eq!(profiler.end_scope(&mut encoder), Err(wgpu_profiler::ScopeError::NoOpenScope));
    }
    // Make sure we can recover from this.
    {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        profiler.begin_scope("open scope", &mut encoder, &device);
        assert_eq!(profiler.end_scope(&mut encoder), Ok(()));
        profiler.resolve_queries(&mut encoder);
    }
    assert_eq!(profiler.end_frame(), Ok(()));
}
