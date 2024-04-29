use wgpu_profiler::{GpuProfiler, GpuProfilerSettings};

use crate::src::{expected_scope, validate_results, Requires};

use super::create_device;

fn nested_scopes(device: &wgpu::Device, queue: &wgpu::Queue) {
    let mut profiler = GpuProfiler::new(GpuProfilerSettings::default()).unwrap();

    let mut encoder0 = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    let mut encoder1 = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    let mut encoder2 = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

    {
        let mut outer_scope = profiler.scope("e0_s0", &mut encoder0, device);
        {
            drop(outer_scope.scoped_compute_pass("e0_s0_c0", device));
            {
                let mut inner_scope = outer_scope.scoped_compute_pass("e0_s0_c1", device);
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
        let mut scope = profiler.scope("e1_s0", &mut encoder1, device);
        {
            drop(scope.scope("e1_s0_s0", device));
            drop(scope.scope("e1_s0_s1", device));
            {
                let mut scope = scope.scope("e1_s0_s2", device);
                drop(scope.scope("e1_s0_s2_s0", device));
            }
        }
    }
    drop(profiler.scope("e2_s0", &mut encoder2, device));
    {
        // Another scope, but with the profiler disabled which should be possible on the fly.
        profiler
            .change_settings(GpuProfilerSettings {
                enable_timer_queries: false,
                ..Default::default()
            })
            .unwrap();
        let mut scope = profiler.scope("e2_s1", &mut encoder0, device);
        {
            let mut scope = scope.scoped_compute_pass("e2_s1_c1", device);
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
            expected_scope(
                "e0_s0",
                Requires::TimestampsInEncoders,
                [
                    expected_scope("e0_s0_c0", Requires::Timestamps, []),
                    expected_scope(
                        "e0_s0_c1",
                        Requires::Timestamps,
                        [
                            expected_scope("e0_s0_c1_s0", Requires::TimestampsInPasses, []),
                            expected_scope(
                                "e0_s0_c1_s1",
                                Requires::TimestampsInPasses,
                                [expected_scope(
                                    "e0_s0_c1_s1_s0",
                                    Requires::TimestampsInPasses,
                                    [
                                        expected_scope(
                                            "e0_s0_c1_s1_s0_s0",
                                            Requires::TimestampsInPasses,
                                            [],
                                        ), //
                                    ],
                                )],
                            ),
                        ],
                    ),
                ],
            ),
            expected_scope(
                "e1_s0",
                Requires::TimestampsInEncoders,
                [
                    expected_scope("e1_s0_s0", Requires::Timestamps, []),
                    expected_scope("e1_s0_s1", Requires::Timestamps, []),
                    expected_scope(
                        "e1_s0_s2",
                        Requires::Timestamps,
                        [
                            expected_scope("e1_s0_s2_s0", Requires::Timestamps, []), //
                        ],
                    ),
                ],
            ),
            expected_scope("e2_s0", Requires::TimestampsInEncoders, []),
        ],
    );
}

#[test]
fn nested_scopes_all_features() {
    let Ok((_, device, queue)) = create_device(GpuProfiler::ALL_WGPU_TIMER_FEATURES) else {
        println!("Skipping test because device doesn't support timer features");
        return;
    };
    nested_scopes(&device, &queue);
}

#[test]
fn nested_scopes_no_pass_features() {
    let (_, device, queue) = create_device(wgpu::Features::TIMESTAMP_QUERY).unwrap();
    nested_scopes(&device, &queue);
}

#[test]
fn nested_scopes_no_features() {
    let (_, device, queue) = create_device(wgpu::Features::empty()).unwrap();
    nested_scopes(&device, &queue);
}
