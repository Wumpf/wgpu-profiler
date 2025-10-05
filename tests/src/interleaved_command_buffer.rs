use wgpu_profiler::{GpuProfiler, GpuProfilerSettings};

use crate::src::{expected_scope, validate_results, validate_results_unordered, Requires};

use super::create_device;

#[test]
fn interleaved_scopes() {
    let (_, device, queue) = create_device(wgpu::Features::TIMESTAMP_QUERY).unwrap();

    let mut profiler = GpuProfiler::new(&device, GpuProfilerSettings::default()).unwrap();

    let mut encoder0 = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    let mut encoder1 = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

    {
        let mut e0_s0 = profiler.scope("e0_s0", &mut encoder0);
        let mut e1_s0 = profiler.scope("e1_s0", &mut encoder1);

        drop(e0_s0.scope("e0_s0_s0"));
        drop(e0_s0.scope("e0_s0_s1"));
        drop(e1_s0.scope("e1_s0_s0"));
    }

    profiler.resolve_queries(&mut encoder0);
    queue.submit([encoder1.finish(), encoder0.finish()]);
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
                "e1_s0",
                Requires::TimestampsInEncoders,
                [expected_scope(
                    "e1_s0_s0",
                    Requires::TimestampsInEncoders,
                    [],
                )],
            ),
            expected_scope(
                "e0_s0",
                Requires::TimestampsInEncoders,
                [
                    expected_scope("e0_s0_s0", Requires::TimestampsInEncoders, []),
                    expected_scope("e0_s0_s1", Requires::TimestampsInEncoders, []),
                ],
            ),
        ],
    );
}

#[test]
fn multithreaded_scopes() {
    let (_, device, queue) = create_device(wgpu::Features::TIMESTAMP_QUERY).unwrap();

    let mut profiler = GpuProfiler::new(&device, GpuProfilerSettings::default()).unwrap();

    const NUM_SCOPES_PER_THREAD: usize = 1000;

    let barrier = std::sync::Barrier::new(2);
    let (command_buffer0, command_buffer1) = std::thread::scope(|thread_scope| {
        let join_handle0 = thread_scope.spawn(|| {
            let mut encoder =
                device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            barrier.wait();

            for i in 0..NUM_SCOPES_PER_THREAD {
                let _ = profiler.scope(format!("e0_s{i}"), &mut encoder);
            }
            encoder.finish()
        });
        let join_handle1 = thread_scope.spawn(|| {
            let mut encoder =
                device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            barrier.wait();

            for i in 0..NUM_SCOPES_PER_THREAD {
                let _ = profiler.scope(format!("e1_s{i}"), &mut encoder);
            }
            encoder.finish()
        });

        (join_handle0.join().unwrap(), join_handle1.join().unwrap())
    });

    let mut resolve_encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    profiler.resolve_queries(&mut resolve_encoder);
    queue.submit([command_buffer0, command_buffer1, resolve_encoder.finish()]);
    profiler.end_frame().unwrap();

    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();

    // Single frame should now be available.
    let frame = profiler
        .process_finished_frame(queue.get_timestamp_period())
        .unwrap();

    // Print entire tree. Useful for debugging the test if it fails!
    println!("{frame:#?}");

    // Both encoders should have produces the scopes, albeit in arbitrary order.
    validate_results_unordered(
        device.features(),
        &frame,
        &(0..NUM_SCOPES_PER_THREAD)
            .map(|i| expected_scope(format!("e0_s{i}"), Requires::TimestampsInEncoders, []))
            .chain(
                (0..NUM_SCOPES_PER_THREAD).map(|i| {
                    expected_scope(format!("e1_s{i}"), Requires::TimestampsInEncoders, [])
                }),
            )
            .collect::<Vec<_>>(),
    );
}
