use wgpu_profiler::{GpuProfiler, GpuProfilerSettings};

use crate::src::{expected_scope, validate_results, Requires};

use super::create_device;

fn nested_scopes(device: &wgpu::Device, queue: &wgpu::Queue) {
    let mut profiler = GpuProfiler::new(device, GpuProfilerSettings::default()).unwrap();

    let mut encoder0 = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    let mut encoder1 = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    let mut encoder2 = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

    {
        let mut outer_scope = profiler.scope("e0_s0", &mut encoder0);
        {
            drop(outer_scope.scoped_compute_pass("e0_s0_c0"));
            {
                let mut inner_scope = outer_scope.scoped_compute_pass("e0_s0_c1");
                {
                    drop(inner_scope.scope("e0_s0_c1_s0"));
                    let mut innermost_scope = inner_scope.scope("e0_s0_c1_s1");
                    {
                        let mut scope = innermost_scope.scope("e0_s0_c1_s1_s0");
                        drop(scope.scope("e0_s0_c1_s1_s0_s0"));
                    }
                }
            }
        }
    }
    // Bunch of interleaved scopes on an encoder.
    {
        let mut scope = profiler.scope("e1_s0", &mut encoder1);
        {
            drop(scope.scope("e1_s0_s0"));
            drop(scope.scope("e1_s0_s1"));
            {
                let mut scope = scope.scope("e1_s0_s2");
                drop(scope.scope("e1_s0_s2_s0"));
            }
        }
    }
    drop(profiler.scope("e2_s0", &mut encoder2));
    {
        // Another scope, but with the profiler disabled which should be possible on the fly.
        profiler
            .change_settings(GpuProfilerSettings {
                enable_timer_queries: false,
                ..Default::default()
            })
            .unwrap();
        let mut scope = profiler.scope("e2_s1", &mut encoder0);
        {
            let mut scope = scope.scoped_compute_pass("e2_s1_c1");
            drop(scope.scope("e2_s1_c1_s0"));
        }
    }

    profiler.resolve_queries(&mut encoder2);
    queue.submit([encoder0.finish(), encoder1.finish(), encoder2.finish()]);
    profiler.end_frame().unwrap();

    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();

    // Single frame should now be available.
    let frame = profiler
        .process_finished_frame(queue.get_timestamp_period())
        .unwrap();

    // Print entire tree. Useful for debugging the test if it fails!
    println!("{frame:#?}");

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
                    expected_scope("e1_s0_s0", Requires::TimestampsInEncoders, []),
                    expected_scope("e1_s0_s1", Requires::TimestampsInEncoders, []),
                    expected_scope(
                        "e1_s0_s2",
                        Requires::TimestampsInEncoders,
                        [
                            expected_scope("e1_s0_s2_s0", Requires::TimestampsInEncoders, []), //
                        ],
                    ),
                ],
            ),
            expected_scope("e2_s0", Requires::TimestampsInEncoders, []),
            expected_scope(
                "e2_s1",
                Requires::Disabled,
                [expected_scope(
                    "e2_s1_c1",
                    Requires::Disabled,
                    [expected_scope("e2_s1_c1_s0", Requires::Disabled, [])],
                )],
            ),
        ],
    );
}

// Note that `TIMESTAMP_QUERY_INSIDE_PASSES` implies support for `TIMESTAMP_QUERY_INSIDE_ENCODERS`.
// But as of writing wgpu allows enabling pass timestamps without encoder timestamps and we should handle this fine as well!

#[test]
fn nested_scopes_timestamp_in_passes_and_encoder_enabled() {
    let Ok((_, device, queue)) = create_device(
        wgpu::Features::TIMESTAMP_QUERY
            | wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES
            | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS,
    ) else {
        println!("Skipping test because device doesn't support TIMESTAMP_QUERY_INSIDE_PASSES");
        return;
    };
    nested_scopes(&device, &queue);
}

#[test]
fn nested_scopes_timestamp_in_passes_enabled() {
    let Ok((_, device, queue)) = create_device(
        wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES,
    ) else {
        println!("Skipping test because device doesn't support TIMESTAMP_QUERY_INSIDE_PASSES");
        return;
    };
    nested_scopes(&device, &queue);
}

#[test]
fn nested_scopes_timestamp_in_encoders_enabled() {
    let Ok((_, device, queue)) = create_device(
        wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS,
    ) else {
        println!("Skipping test because device doesn't support TIMESTAMP_QUERY_INSIDE_ENCODERS");
        return;
    };
    nested_scopes(&device, &queue);
}

#[test]
fn nested_scopes_timestamp_enabled() {
    let Ok((_, device, queue)) = create_device(wgpu::Features::TIMESTAMP_QUERY) else {
        println!("Skipping test because device doesn't support TIMESTAMP_QUERY");
        return;
    };
    nested_scopes(&device, &queue);
}

#[test]
fn nested_scopes_no_features() {
    let (_, device, queue) = create_device(wgpu::Features::empty()).unwrap();
    nested_scopes(&device, &queue);
}
