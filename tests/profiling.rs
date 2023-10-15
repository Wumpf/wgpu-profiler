use wgpu_profiler::{scope::Scope, GpuProfiler, GpuProfilerSettings, GpuTimerScopeResult};

mod utils;

#[derive(Debug)]
enum Requires {
    Timestamps,
    TimestampsInPasses,
}

#[derive(Debug)]
struct ExpectedScope(&'static str, Requires, &'static [ExpectedScope]);

fn validate_results(
    features: wgpu::Features,
    result: &[GpuTimerScopeResult],
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

fn nested_scopes(device: &wgpu::Device, queue: &wgpu::Queue) {
    let mut profiler = GpuProfiler::new(GpuProfilerSettings::default()).unwrap();

    let mut encoder0 = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    let mut encoder1 = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    let mut encoder2 = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

    {
        let mut scope = Scope::start("e0_s0", &mut profiler, &mut encoder0, device);
        {
            drop(scope.scoped_compute_pass(
                "e0_s0_c0",
                device,
                &wgpu::ComputePassDescriptor::default(),
            ));
            let mut scope = scope.scoped_compute_pass(
                "e0_s0_c1",
                device,
                &wgpu::ComputePassDescriptor::default(),
            );
            {
                drop(scope.scope("e0_s0_c1_s0", device));
                let mut scope = scope.scope("e0_s0_c1_s1", device);
                {
                    let mut scope = scope.scope("e0_s0_c1_s1_s0", device);
                    drop(scope.scope("e0_s0_c1_s1_s0_s0", device));
                }
            }
        }
    }
    // Bunch of interleaved scopes on an encoder.
    {
        let mut scope = Scope::start("e1_s0", &mut profiler, &mut encoder1, device);
        {
            drop(scope.scope("e1_s0_s0", device));
            drop(scope.scope("e1_s0_s1", device));
            {
                let mut scope = scope.scope("e1_s0_s2", device);
                drop(scope.scope("e1_s0_s2_s0", device));
            }
        }
    }
    drop(Scope::start("e2_s0", &mut profiler, &mut encoder2, device));
    {
        // Another scope, but with the profiler disabled which should be possible on the fly.
        profiler
            .change_settings(GpuProfilerSettings {
                enable_timer_scopes: false,
                ..Default::default()
            })
            .unwrap();
        let mut scope = Scope::start("e2_s1", &mut profiler, &mut encoder0, device);
        {
            let mut scope = scope.scoped_compute_pass(
                "e2_s1_c1",
                device,
                &wgpu::ComputePassDescriptor::default(),
            );
            drop(scope.scope("e2_s1_c1_s0", device));
        }
    }

    profiler.resolve_queries(&mut encoder2);
    queue.submit([encoder0.finish(), encoder1.finish(), encoder2.finish()]);
    profiler.end_frame().unwrap();

    device.poll(wgpu::Maintain::Wait);

    // Single frame should now be available.
    let frame = profiler
        .process_finished_frame(queue.get_timestamp_period())
        .unwrap();

    // Check if the frame gives us the expected nesting of timer scopes.
    validate_results(
        device.features(),
        &frame,
        &[
            ExpectedScope(
                "e0_s0",
                Requires::Timestamps,
                &[
                    ExpectedScope("e0_s0_c0", Requires::TimestampsInPasses, &[]),
                    ExpectedScope(
                        "e0_s0_c1",
                        Requires::TimestampsInPasses,
                        &[
                            ExpectedScope("e0_s0_c1_s0", Requires::TimestampsInPasses, &[]),
                            ExpectedScope(
                                "e0_s0_c1_s1",
                                Requires::TimestampsInPasses,
                                &[ExpectedScope(
                                    "e0_s0_c1_s1_s0",
                                    Requires::TimestampsInPasses,
                                    &[
                                        ExpectedScope(
                                            "e0_s0_c1_s1_s0_s0",
                                            Requires::TimestampsInPasses,
                                            &[],
                                        ), //
                                    ],
                                )],
                            ),
                        ],
                    ),
                ],
            ),
            ExpectedScope(
                "e1_s0",
                Requires::Timestamps,
                &[
                    ExpectedScope("e1_s0_s0", Requires::Timestamps, &[]),
                    ExpectedScope("e1_s0_s1", Requires::Timestamps, &[]),
                    ExpectedScope(
                        "e1_s0_s2",
                        Requires::Timestamps,
                        &[
                            ExpectedScope("e1_s0_s2_s0", Requires::Timestamps, &[]), //
                        ],
                    ),
                ],
            ),
            ExpectedScope("e2_s0", Requires::Timestamps, &[]),
        ],
    );
}

#[test]
fn nested_scopes_all_features() {
    let (_, device, queue) = utils::create_device(GpuProfiler::ALL_WGPU_TIMER_FEATURES);
    nested_scopes(&device, &queue);
}

#[test]
fn nested_scopes_no_pass_features() {
    let (_, device, queue) = utils::create_device(wgpu::Features::TIMESTAMP_QUERY);
    nested_scopes(&device, &queue);
}

#[test]
fn nested_scopes_no_features() {
    let (_, device, queue) = utils::create_device(wgpu::Features::empty());
    nested_scopes(&device, &queue);
}

// TODO: interleaving of scope begin_end & multithreading is not yet possible!
