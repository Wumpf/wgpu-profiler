pub fn create_device(features: wgpu::Features) -> (wgpu::Backend, wgpu::Device, wgpu::Queue) {
    async fn create_default_device_async(features: wgpu::Features) -> (wgpu::Backend, wgpu::Device, wgpu::Queue) {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
        let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions::default()).await.unwrap();
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    features,
                    ..Default::default()
                },
                None,
            )
            .await
            .unwrap();
        (adapter.get_info().backend, device, queue)
    }

    futures_lite::future::block_on(create_default_device_async(features))
}
