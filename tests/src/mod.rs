use wgpu::RequestDeviceError;

mod dropped_frame_handling;
mod errors;
mod interleaved_command_buffer;
mod nested_scopes;

pub fn create_device(
    features: wgpu::Features,
) -> Result<(wgpu::Backend, wgpu::Device, wgpu::Queue), RequestDeviceError> {
    async fn create_default_device_async(
        features: wgpu::Features,
    ) -> Result<(wgpu::Backend, wgpu::Device, wgpu::Queue), RequestDeviceError> {
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
                    required_features: features,
                    ..Default::default()
                },
                None,
            )
            .await?;
        Ok((adapter.get_info().backend, device, queue))
    }

    futures_lite::future::block_on(create_default_device_async(features))
}

#[derive(Debug)]
enum Requires {
    Timestamps,
    TimestampsInPasses,
}

#[derive(Debug)]
struct ExpectedScope(String, Requires, Vec<ExpectedScope>);

fn expected_scope(
    label: impl Into<String>,
    requires: Requires,
    children: impl Into<Vec<ExpectedScope>>,
) -> ExpectedScope {
    ExpectedScope(label.into(), requires, children.into())
}

fn validate_results(
    features: wgpu::Features,
    result: &[wgpu_profiler::GpuTimerQueryResult],
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
        validate_results(features, &result.nested_queries, &expected.2);
    }
}

fn validate_results_unordered(
    features: wgpu::Features,
    result: &[wgpu_profiler::GpuTimerQueryResult],
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

    let mut expected_labels = std::collections::HashSet::<String>::from_iter(
        expected.iter().map(|expected| expected.0.clone()),
    );

    for (result, expected) in result.iter().zip(expected.iter()) {
        assert!(expected_labels.remove(&result.label));
        validate_results(features, &result.nested_queries, &expected.2);
    }
}
