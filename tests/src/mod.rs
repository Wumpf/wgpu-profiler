mod dropped_frame_handling;
mod errors;
mod interleaved_command_buffer;
mod nested_scopes;

pub fn create_device(features: wgpu::Features) -> (wgpu::Backend, wgpu::Device, wgpu::Queue) {
    async fn create_default_device_async(
        features: wgpu::Features,
    ) -> (wgpu::Backend, wgpu::Device, wgpu::Queue) {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY, // Workaround for wgl having issues with parallel device destruction.
            ..Default::default()
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .unwrap();
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

#[derive(Debug)]
enum Requires {
    Timestamps,
    TimestampsInPasses,
}

#[derive(Debug)]
struct ExpectedScope(&'static str, Requires, &'static [ExpectedScope]);

fn validate_results(
    features: wgpu::Features,
    result: &[wgpu_profiler::GpuTimerScopeResult],
    expected: &[ExpectedScope],
) {
    let expected = expected
        .iter()
        .filter(|expected| match expected.1 {
            Requires::Timestamps => features.contains(wgpu::Features::TIMESTAMP_QUERY),
            Requires::TimestampsInPasses => {
                features.contains(wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES)
            }
        })
        .collect::<Vec<_>>();

    assert_eq!(
        result.len(),
        expected.len(),
        "result: {result:?}\nexpected: {expected:?}"
    );
    for (result, expected) in result.iter().zip(expected.iter()) {
        assert_eq!(result.label, expected.0);
        validate_results(features, &result.nested_scopes, &expected.2);
    }
}
