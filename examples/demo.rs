use std::{borrow::Cow, sync::Arc};
use wgpu_profiler::{GpuProfiler, GpuProfilerSettings, GpuTimerQueryResult};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::EventLoop,
    keyboard::{KeyCode, PhysicalKey},
};

#[cfg(feature = "puffin")]
// Since the timing information we get from WGPU may be several frames behind the CPU, we can't report these frames to
// the singleton returned by `puffin::GlobalProfiler::lock`. Instead, we need our own `puffin::GlobalProfiler` that we
// can be several frames behind puffin's main global profiler singleton.
static PUFFIN_GPU_PROFILER: std::sync::LazyLock<std::sync::Mutex<puffin::GlobalProfiler>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(puffin::GlobalProfiler::default()));

fn scopes_to_console_recursive(results: &[GpuTimerQueryResult], indentation: u32) {
    for scope in results {
        if indentation > 0 {
            print!("{:<width$}", "|", width = 4);
        }

        if let Some(time) = &scope.time {
            println!(
                "{:.3}μs - {}",
                (time.end - time.start) * 1000.0 * 1000.0,
                scope.label
            );
        } else {
            println!("n/a - {}", scope.label);
        }

        if !scope.nested_queries.is_empty() {
            scopes_to_console_recursive(&scope.nested_queries, indentation + 1);
        }
    }
}

fn console_output(results: &Option<Vec<GpuTimerQueryResult>>, enabled_features: wgpu::Features) {
    profiling::scope!("console_output");
    print!("\x1B[2J\x1B[1;1H"); // Clear terminal and put cursor to first row first column
    println!("Welcome to wgpu_profiler demo!");
    println!();
    println!("Enabled device features: {enabled_features:?}");
    println!();
    println!(
        "Press space to write out a trace file that can be viewed in chrome's chrome://tracing"
    );
    println!();
    match results {
        Some(results) => {
            scopes_to_console_recursive(results, 0);
        }
        None => println!("No profiling results available yet!"),
    }
}

#[derive(Default)]
struct State {
    window: Option<Arc<winit::window::Window>>,
    gfx_state: Option<GfxState>,
    latest_profiler_results: Option<Vec<GpuTimerQueryResult>>,
}

struct GfxState {
    surface: wgpu::Surface<'static>,
    surface_desc: wgpu::SurfaceConfiguration,
    device: wgpu::Device,
    queue: wgpu::Queue,
    render_pipeline: wgpu::RenderPipeline,
    profiler: GpuProfiler,
}

impl GfxState {
    async fn new(window: &Arc<winit::window::Window>) -> Self {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let surface = instance
            .create_surface(window.clone())
            .expect("Failed to create surface.");
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
            .request_device(&wgpu::DeviceDescriptor {
                required_features: adapter.features() & GpuProfiler::ALL_WGPU_TIMER_FEATURES,
                ..Default::default()
            })
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

        let surface_desc = wgpu::SurfaceConfiguration {
            present_mode: wgpu::PresentMode::AutoVsync,
            ..surface
                .get_default_config(&adapter, size.width, size.height)
                .unwrap()
        };
        surface.configure(&device, &surface_desc);

        let swapchain_format = surface_desc.format;

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(swapchain_format.into())],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // Create a new profiler instance.
        #[cfg(feature = "tracy")]
        let profiler = GpuProfiler::new_with_tracy_client(
            GpuProfilerSettings::default(),
            adapter.get_info().backend,
            &device,
            &queue,
        )
        .unwrap_or_else(|err| match err {
            wgpu_profiler::CreationError::TracyClientNotRunning
            | wgpu_profiler::CreationError::TracyGpuContextCreationError(_) => {
                println!("Failed to connect to Tracy. Continuing without Tracy integration.");
                GpuProfiler::new(&device, GpuProfilerSettings::default())
                    .expect("Failed to create profiler")
            }
            _ => {
                panic!("Failed to create profiler: {err}");
            }
        });

        #[cfg(not(feature = "tracy"))]
        let profiler = GpuProfiler::new(&device, GpuProfilerSettings::default())
            .expect("Failed to create profiler");

        Self {
            surface,
            surface_desc,
            device,
            queue,
            render_pipeline,
            profiler,
        }
    }
}

impl ApplicationHandler<()> for State {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.window.is_none() {
            let window = Arc::new(
                event_loop
                    .create_window(
                        winit::window::WindowAttributes::default().with_title("wgpu_profiler demo"),
                    )
                    .expect("Failed to create window"),
            );

            // Future versions of winit are supposed to be able to return a Future here for web support:
            // https://github.com/rust-windowing/winit/issues/3626#issuecomment-2097916252
            let gfx_state = futures_lite::future::block_on(GfxState::new(&window));

            self.window = Some(window);
            self.gfx_state = Some(gfx_state);
        }
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let Some(GfxState {
            surface,
            surface_desc,
            device,
            queue,
            render_pipeline,
            profiler,
        }) = self.gfx_state.as_mut()
        else {
            return;
        };

        match event {
            WindowEvent::Resized(size) => {
                if size.width > 0 && size.height > 0 {
                    surface_desc.width = size.width;
                    surface_desc.height = size.height;
                    surface.configure(device, surface_desc);
                }
            }
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }

            WindowEvent::RedrawRequested => {
                profiling::scope!("Redraw Requested");

                let frame = surface
                    .get_current_texture()
                    .expect("Failed to acquire next surface texture");
                let frame_view = frame
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());
                let mut encoder =
                    device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

                draw(profiler, &mut encoder, &frame_view, render_pipeline);

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
                self.latest_profiler_results =
                    profiler.process_finished_frame(queue.get_timestamp_period());
                console_output(&self.latest_profiler_results, device.features());
                #[cfg(feature = "puffin")]
                {
                    let mut gpu_profiler = PUFFIN_GPU_PROFILER.lock().unwrap();
                    wgpu_profiler::puffin::output_frame_to_puffin(
                        &mut gpu_profiler,
                        self.latest_profiler_results.as_deref().unwrap_or_default(),
                    );
                    gpu_profiler.new_frame();
                }
            }

            WindowEvent::KeyboardInput {
                event:
                    winit::event::KeyEvent {
                        physical_key: PhysicalKey::Code(keycode),
                        ..
                    },
                ..
            } => match keycode {
                KeyCode::Escape => {
                    event_loop.exit();
                }
                KeyCode::Space => {
                    if let Some(profile_data) = &self.latest_profiler_results {
                        wgpu_profiler::chrometrace::write_chrometrace(
                            std::path::Path::new("trace.json"),
                            profile_data,
                        )
                        .expect("Failed to write trace.json");
                    }
                }
                _ => {}
            },
            _ => (),
        };
    }

    fn about_to_wait(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop) {
        if let Some(window) = &self.window {
            // Continuous rendering!
            window.request_redraw();
        }
    }
}

fn draw(
    profiler: &GpuProfiler,
    encoder: &mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
    render_pipeline: &wgpu::RenderPipeline,
) {
    // Create a new profiling scope that we nest the other scopes in.
    let mut scope = profiler.scope("rendering", encoder);
    // For demonstration purposes we divide our scene into two render passes.
    {
        // Once we created a scope, we can use it to create nested scopes within.
        // Note that the resulting scope fully owns the render pass.
        // But just as before, it behaves like a transparent wrapper, so you can use it just like a normal render pass.
        let mut rpass = scope.scoped_render_pass(
            "render pass top",
            wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            },
        );

        rpass.set_pipeline(render_pipeline);

        // Sub-scopes within the pass only work if wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES is enabled.
        // If this feature is lacking, no timings will be taken.
        {
            let mut rpass = rpass.scope("fractal 0");
            rpass.draw(0..6, 0..1);
        };
        {
            let mut rpass = rpass.scope("fractal 1");
            rpass.draw(0..6, 1..2);
        }
    }
    {
        // It's also possible to take timings by hand, manually calling `begin_query` and `end_query`.
        // This is generally not recommended as it's very easy to mess up by accident :)
        let pass_scope = profiler
            .begin_pass_query("render pass bottom", scope.recorder)
            .with_parent(scope.scope.as_ref());
        let mut rpass = scope
            .recorder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: pass_scope.render_pass_timestamp_writes(),
            });

        rpass.set_pipeline(render_pipeline);

        // Similarly, you can manually manage nested scopes within a render pass.
        // Again, to do any actual timing, you need to enable wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES.
        {
            let query = profiler
                .begin_query("fractal 2", &mut rpass)
                .with_parent(Some(&pass_scope));
            rpass.draw(0..6, 2..3);

            // Don't forget to end the query!
            profiler.end_query(&mut rpass, query);
        }
        // Another variant is to use `ManualOwningScope`, forming a middle ground between no scope helpers and fully automatic scope closing.
        let mut rpass = {
            let mut rpass = profiler.manual_owning_scope("fractal 3", rpass);
            rpass.draw(0..6, 3..4);

            // Don't forget to end the scope.
            // Ending a `ManualOwningScope` will return the pass or encoder it owned.
            rpass.end_query()
        };

        // Don't forget to end the scope.
        profiler.end_query(&mut rpass, pass_scope);
    }
}

fn main() {
    #[cfg(feature = "tracy")]
    tracy_client::Client::start();

    //env_logger::init_from_env(env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "warn"));
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);

    #[cfg(feature = "puffin")]
    let (_cpu_server, _gpu_server) = {
        puffin::set_scopes_on(true);
        let cpu_server =
            puffin_http::Server::new(&format!("0.0.0.0:{}", puffin_http::DEFAULT_PORT)).unwrap();
        let gpu_server = puffin_http::Server::new_custom(
            &format!("0.0.0.0:{}", puffin_http::DEFAULT_PORT + 1),
            |sink| PUFFIN_GPU_PROFILER.lock().unwrap().add_sink(sink),
            |id| _ = PUFFIN_GPU_PROFILER.lock().unwrap().remove_sink(id),
        );
        (cpu_server, gpu_server)
    };
    let _ = event_loop.run_app(&mut State::default());
}
