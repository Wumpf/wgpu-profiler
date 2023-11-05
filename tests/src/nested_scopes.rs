use wgpu_profiler::{GpuProfiler, GpuProfilerSettings};

use crate::src::{validate_results, ExpectedScope, Requires};

use super::create_device;

fn nested_scopes(device: &wgpu::Device, queue: &wgpu::Queue) {
    let mut profiler = GpuProfiler::new(GpuProfilerSettings::default()).unwrap();

    let mut encoder0 = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    let mut encoder1 = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    let mut encoder2 = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

    {
        let mut outer_scope =
            wgpu_profiler::Scope::start("e0_s0", &profiler, &mut encoder0, device);
        {
            drop(outer_scope.scoped_compute_pass(
                "e0_s0_c0",
                device,
                &wgpu::ComputePassDescriptor::default(),
            ));
            {
                let mut inner_scope = outer_scope.scoped_compute_pass(
                    "e0_s0_c1",
                    device,
                    &wgpu::ComputePassDescriptor::default(),
                );
                {
                    drop(inner_scope.scope("e0_s0_c1_s0", device));
                    let mut innermost_scope = inner_scope.scope("e0_s0_c1_s1", device);
                    {
                        let mut scope = innermost_scope.scope("e0_s0_c1_s1_s0", device);
                        drop(scope.scope("e0_s0_c1_s1_s0_s0", device));
                    }
                }
            }
        }
    }
    // Bunch of interleaved scopes on an encoder.
    {
        let mut scope = wgpu_profiler::Scope::start("e1_s0", &profiler, &mut encoder1, device);
        {
            drop(scope.scope("e1_s0_s0", device));
            drop(scope.scope("e1_s0_s1", device));
            {
                let mut scope = scope.scope("e1_s0_s2", device);
                drop(scope.scope("e1_s0_s2_s0", device));
            }
        }
    }
    drop(wgpu_profiler::Scope::start(
        "e2_s0",
        &profiler,
        &mut encoder2,
        device,
    ));
    {
        // Another scope, but with the profiler disabled which should be possible on the fly.
        profiler
            .change_settings(GpuProfilerSettings {
                enable_timer_scopes: false,
                ..Default::default()
            })
            .unwrap();
        let mut scope = wgpu_profiler::Scope::start("e2_s1", &profiler, &mut encoder0, device);
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

    // Print entire tree. Useful for debugging the test if it fails!
    println!("{:#?}", frame);

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
    let (_, device, queue) = create_device(GpuProfiler::ALL_WGPU_TIMER_FEATURES);
    nested_scopes(&device, &queue);
}

#[test]
fn nested_scopes_no_pass_features() {
    let (_, device, queue) = create_device(wgpu::Features::TIMESTAMP_QUERY);
    nested_scopes(&device, &queue);
}

#[test]
fn nested_scopes_no_features() {
    let (_, device, queue) = create_device(wgpu::Features::empty());
    nested_scopes(&device, &queue);
}