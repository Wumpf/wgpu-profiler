use wgpu_profiler::{GpuProfiler, GpuProfilerSettings};

use crate::src::{validate_results, ExpectedScope, Requires};

use super::create_device;

#[test]
fn interleaved_scopes() {
    let (_, device, queue) = create_device(wgpu::Features::TIMESTAMP_QUERY);

    let mut profiler = GpuProfiler::new(GpuProfilerSettings::default()).unwrap();

    let mut encoder0 = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    let mut encoder1 = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

    {
        let mut e0_s0 = wgpu_profiler::Scope::start("e0_s0", &profiler, &mut encoder0, &device);
        let mut e1_s0 = wgpu_profiler::Scope::start("e1_s0", &profiler, &mut encoder1, &device);

        drop(e0_s0.scope("e0_s0_s0", &device));
        drop(e0_s0.scope("e0_s0_s1", &device));
        drop(e1_s0.scope("e1_s0_s0", &device));
    }

    profiler.resolve_queries(&mut encoder0);
    queue.submit([encoder1.finish(), encoder0.finish()]);
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
                "e1_s0",
                Requires::Timestamps,
                &[ExpectedScope("e1_s0_s0", Requires::Timestamps, &[])],
            ),
            ExpectedScope(
                "e0_s0",
                Requires::Timestamps,
                &[
                    ExpectedScope("e0_s0_s0", Requires::Timestamps, &[]),
                    ExpectedScope("e0_s0_s1", Requires::Timestamps, &[]),
                ],
            ),
        ],
    );
}
