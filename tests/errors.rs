// Create a default device *without* the TIMESTAMP_QUERY feature.
fn create_default_device() -> (wgpu::Device, wgpu::Queue) {
    async fn create_default_device_async() -> (wgpu::Device, wgpu::Queue) {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
        let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions::default()).await.unwrap();
        adapter.request_device(&wgpu::DeviceDescriptor::default(), None).await.unwrap()
    }

    futures_lite::future::block_on(create_default_device_async())
}

#[test]
fn end_frame_errors() {
    let (device, queue) = create_default_device();

    // Unclosed scope
    {
        let mut profiler = wgpu_profiler::GpuProfiler::new(1, queue.get_timestamp_period(), device.features());
        {
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            profiler.begin_scope("open scope", &mut encoder, &device);
            profiler.resolve_queries(&mut encoder);
        }

        assert_eq!(
            profiler.end_frame(),
            Err(wgpu_profiler::GpuProfilerError::UnclosedScopesAtFrameEnd(vec!["open scope".to_string()]))
        );
    }

    // Unresolved scope
    {
        let mut profiler = wgpu_profiler::GpuProfiler::new(1, queue.get_timestamp_period(), device.features());
        {
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            profiler.begin_scope("open scope", &mut encoder, &device);
            profiler.end_scope(&mut encoder);
        }

        assert_eq!(profiler.end_frame(), Err(wgpu_profiler::GpuProfilerError::UnresolvedQueriesAtFrameEnd(2)));
    }
}
