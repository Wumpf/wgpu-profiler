fn create_device(timestamps_enabled: bool) -> (wgpu::Adapter, wgpu::Device, wgpu::Queue) {
    async fn create_default_device_async(timestamps_enabled: bool) -> (wgpu::Adapter, wgpu::Device, wgpu::Queue) {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
        let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions::default()).await.unwrap();
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    features: if timestamps_enabled {
                        wgpu::Features::TIMESTAMP_QUERY
                    } else {
                        wgpu::Features::empty()
                    },
                    ..Default::default()
                },
                None,
            )
            .await
            .unwrap();
        (adapter, device, queue)
    }

    futures_lite::future::block_on(create_default_device_async(timestamps_enabled))
}

#[test]
fn end_frame_unclosed_scope() {
    // Doesn't require the TIMESTAMP_QUERY feature.
    let (adapter, device, queue) = create_device(false);

    let mut profiler = wgpu_profiler::GpuProfiler::new(&adapter, &device, &queue, 1);
    {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        profiler.begin_scope("open scope", &mut encoder, &device);
        profiler.resolve_queries(&mut encoder);
    }

    assert_eq!(
        profiler.end_frame(),
        Err(wgpu_profiler::GpuProfilerError::UnclosedScopesAtFrameEnd(vec!["open scope".to_string()]))
    );

    // Make sure we can recover from this.
    {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        profiler.end_scope(&mut encoder);
        profiler.resolve_queries(&mut encoder);
    }
    assert_eq!(profiler.end_frame(), Ok(()));
}

#[test]
fn end_frame_unresolved_scope() {
    // Requires the TIMESTAMP_QUERY feature currently since we don't track scope resolving otherwise.
    let (adapter, device, queue) = create_device(true);

    let mut profiler = wgpu_profiler::GpuProfiler::new(&adapter, &device, &queue, 1);
    {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        profiler.begin_scope("open scope", &mut encoder, &device);
        profiler.end_scope(&mut encoder);
    }

    assert_eq!(profiler.end_frame(), Err(wgpu_profiler::GpuProfilerError::UnresolvedQueriesAtFrameEnd(2)));

    // Make sure we can recover from this!
    {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        profiler.resolve_queries(&mut encoder);
    }
    assert_eq!(profiler.end_frame(), Ok(()));
}
