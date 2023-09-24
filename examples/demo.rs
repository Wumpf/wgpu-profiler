use std::borrow::Cow;
use wgpu_profiler::*;
use winit::{
    event::{Event, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::Window,
};

fn scopes_to_console_recursive(results: &[GpuTimerScopeResult], indentation: u32) {
    for scope in results {
        if indentation > 0 {
            print!("{:<width$}", "|", width = 4);
        }

        println!("{:.3}μs - {}", (scope.time.end - scope.time.start) * 1000.0 * 1000.0, scope.label);

        if !scope.nested_scopes.is_empty() {
            scopes_to_console_recursive(&scope.nested_scopes, indentation + 1);
        }
    }
}

fn console_output(results: &Option<Vec<GpuTimerScopeResult>>, enabled_features: wgpu::Features) {
    profiling::scope!("console_output");
    print!("\x1B[2J\x1B[1;1H"); // Clear terminal and put cursor to first row first column
    println!("Welcome to wgpu_profiler demo!");
    println!();
    println!("Enabled device features: {:?}", enabled_features);
    println!();
    println!("Press space to write out a trace file that can be viewed in chrome's chrome://tracing");
    println!();
    match results {
        Some(results) => {
            scopes_to_console_recursive(results, 0);
        }
        None => println!("No profiling results available yet!"),
    }
}

async fn run(event_loop: EventLoop<()>, window: Window) {
    let size = window.inner_size();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        dx12_shader_compiler: wgpu::Dx12Compiler::default(),
    });
    let surface = unsafe { instance.create_surface(&window) }.expect("Failed to create surface.");
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        })
        .await
        .expect("Failed to find an appropriate adapter");

    dbg!(adapter.features());

    let (device, queue) = adapter
        .request_device(
            &wgpu::DeviceDescriptor {
                label: None,
                features: adapter.features() & GpuProfiler::ALL_WGPU_TIMER_FEATURES,
                limits: wgpu::Limits::default(),
            },
            None,
        )
        .await
        .expect("Failed to create device");

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("shader.wgsl"))),
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[],
        push_constant_ranges: &[],
    });

    let swapchain_format = *surface.get_capabilities(&adapter).formats.first().unwrap();

    let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: "vs_main",
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: "fs_main",
            targets: &[Some(swapchain_format.into())],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
    });

    let mut sc_desc = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: swapchain_format,
        width: size.width,
        height: size.height,
        // By using the Fifo mode we ensure that CPU waits for GPU, thus we won't have an arbitrary amount of frames in flight that may be discarded.
        // Profiler works just fine in any other mode, but keep in mind that this can mean that it would need to buffer up many more frames until the first results are back.
        present_mode: wgpu::PresentMode::Fifo,
        alpha_mode: wgpu::CompositeAlphaMode::Auto,
        view_formats: vec![swapchain_format],
    };

    surface.configure(&device, &sc_desc);

    // Create a new profiler instance.
    #[cfg(feature = "tracy")]
    let mut profiler = GpuProfiler::new_with_tracy_client(GpuProfilerSettings::default(), adapter.get_info().backend, &device, &queue)
        .unwrap_or_else(|err| match err {
            CreationError::TracyClientNotRunning | CreationError::TracyGpuContextCreationError(_) => {
                println!("Failed to connect to Tracy. Continuing without Tracy integration.");
                GpuProfiler::new(GpuProfilerSettings::default()).expect("Failed to create profiler")
            }
            _ => {
                panic!("Failed to create profiler: {}", err);
            }
        });
    #[cfg(not(feature = "tracy"))]
    let mut profiler = GpuProfiler::new(GpuProfilerSettings::default()).expect("Failed to create profiler");

    let mut latest_profiler_results = None;

    event_loop.run(move |event, _, control_flow| {
        // Have the closure take ownership of the resources.
        // `event_loop.run` never returns, therefore we must do this to ensure
        // the resources are properly cleaned up.
        let _ = (&instance, &adapter, &shader, &pipeline_layout);

        *control_flow = ControlFlow::Poll;
        match event {
            Event::WindowEvent {
                event: WindowEvent::Resized(size),
                ..
            } => {
                if size.width > 0 && size.height > 0 {
                    sc_desc.width = size.width;
                    sc_desc.height = size.height;
                    surface.configure(&device, &sc_desc);
                }
            }
            Event::MainEventsCleared => {
                // Continuos rendering!
                window.request_redraw();
            }
            Event::RedrawRequested(_) => {
                profiling::scope!("Redraw Requested");

                let frame = surface.get_current_texture().expect("Failed to acquire next surface texture");
                let frame_view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
                let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

                wgpu_profiler!("rendering", &mut profiler, &mut encoder, &device, {
                    profiling::scope!("Rendering");

                    let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: None,
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &frame_view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color {
                                    r: 100.0 / 255.0,
                                    g: 149.0 / 255.0,
                                    b: 237.0 / 255.0,
                                    a: 1.0,
                                }),
                                store: true,
                            },
                        })],
                        depth_stencil_attachment: None,
                    });

                    // Obviously all the following draw calls could be collapsed, but they are separated to illustrate the different profiler scopes.

                    // You can profile using a macro.
                    wgpu_profiler!("fractal 0", &mut profiler, &mut rpass, &device, {
                        rpass.set_pipeline(&render_pipeline);
                        rpass.draw(0..6, 0..1);
                    });
                    // ... or a scope object
                    {
                        let mut rpass = wgpu_profiler::scope::Scope::start("fractal 1", &mut profiler, &mut rpass, &device);
                        rpass.draw(0..6, 1..2);
                    }
                    // ... or simply manually
                    {
                        profiler.begin_scope("fractal 2", &mut rpass, &device);
                        rpass.draw(0..6, 2..3);
                        profiler.end_scope(&mut rpass).unwrap();
                    }
                    // ... or a scope object that takes ownership of the pass
                    {
                        let mut scoped_pass = wgpu_profiler::scope::OwningScope::start("fractal 3", &mut profiler, rpass, &device);
                        scoped_pass.draw(0..6, 3..4);
                    }
                });

                // Resolves any queries that might be in flight.
                profiler.resolve_queries(&mut encoder);

                {
                    profiling::scope!("Submit");
                    queue.submit(Some(encoder.finish()));
                }
                {
                    profiling::scope!("Present");
                    frame.present();
                }

                profiling::finish_frame!();

                // Signal to the profiler that the frame is finished.
                profiler.end_frame().unwrap();
                // Query for oldest finished frame (this is almost certainly not the one we just submitted!) and display results in the command line.
                if let Some(results) = profiler.process_finished_frame(queue.get_timestamp_period()) {
                    latest_profiler_results = Some(results);
                }
                console_output(&latest_profiler_results, device.features());
            }
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => {
                    *control_flow = ControlFlow::Exit;
                }
                WindowEvent::KeyboardInput {
                    input:
                        winit::event::KeyboardInput {
                            virtual_keycode: Some(keycode),
                            ..
                        },
                    ..
                } => match keycode {
                    VirtualKeyCode::Escape => *control_flow = ControlFlow::Exit,
                    VirtualKeyCode::Space => {
                        if let Some(profile_data) = &latest_profiler_results {
                            wgpu_profiler::chrometrace::write_chrometrace(std::path::Path::new("trace.json"), profile_data)
                                .expect("Failed to write trace.json");
                        }
                    }
                    _ => {}
                },
                _ => {}
            },
            _ => {}
        }
    });
}

fn main() {
    tracy_client::Client::start();
    //env_logger::init_from_env(env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "warn"));
    let event_loop = EventLoop::new();
    let window = winit::window::WindowBuilder::new().build(&event_loop).unwrap();
    futures_lite::future::block_on(run(event_loop, window));
}
