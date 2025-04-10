/*!

Easy to use profiler scopes for [wgpu](https://github.com/gfx-rs/wgpu) using timer queries.

`wgpu_profiler` manages all the necessary [`wgpu::QuerySet`] and [`wgpu::Buffer`] behind the scenes
and allows you to create to create timer scopes with minimal overhead!

# How to use

```
use wgpu_profiler::*;

# async fn wgpu_init() -> (wgpu::Instance, wgpu::Adapter, wgpu::Device, wgpu::Queue) {
    # let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    # let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions::default()).await.unwrap();
    # let (device, queue) = adapter
    #     .request_device(&wgpu::DeviceDescriptor {
    #         required_features: wgpu::Features::TIMESTAMP_QUERY,
    #         ..Default::default()
    #     })
    #     .await
    #     .unwrap();
    # (instance, adapter, device, queue)
# }
# let (instance, adapter, device, queue) = futures_lite::future::block_on(wgpu_init());
# let cs_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
#     label: None,
#     source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(include_str!("../examples/compute_shader.wgsl"))),
# });
# let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
#        label: None,
#        layout: None,
#        module: &cs_module,
#        entry_point: Some("main"),
#        compilation_options: wgpu::PipelineCompilationOptions::default(),
#        cache: None,
#    });
// ...

let mut profiler = GpuProfiler::new(&device, GpuProfilerSettings::default()).unwrap();

// ...

# let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
{
    // You can now open profiling scopes on any encoder or pass:
    let mut scope = profiler.scope("name of your scope", &mut encoder);

    // Scopes can be nested arbitrarily!
    let mut nested_scope = scope.scope("nested!");

    // Scopes on encoders can be used to easily create profiled passes!
    let mut compute_pass = nested_scope.scoped_compute_pass("profiled compute");


    // Scopes expose the underlying encoder or pass they wrap:
    compute_pass.set_pipeline(&pipeline);
    // ...

    // Scopes created this way are automatically closed when dropped.
}

// Wgpu-profiler needs to insert buffer copy commands.
profiler.resolve_queries(&mut encoder);
# drop(encoder);

// ...

// And finally, to end a profiling frame, call `end_frame`.
// This does a few checks and will let you know if something is off!
profiler.end_frame().unwrap();

// Retrieving the oldest available frame and writing it out to a chrome trace file.
if let Some(profiling_data) = profiler.process_finished_frame(queue.get_timestamp_period()) {
    # let button_pressed = false;
    // You usually want to write to disk only under some condition, e.g. press of a key.
    if button_pressed {
        wgpu_profiler::chrometrace::write_chrometrace(
            std::path::Path::new("mytrace.json"), &profiling_data);
    }
}
```
Check also the [Example](https://github.com/Wumpf/wgpu-profiler/blob/main/examples/demo.rs) where everything can be seen in action.

## Tracy integration

If you want to use [tracy](https://github.com/wolfpld/tracy) for profiling, you can enable the `tracy` feature.

This adds `wgpu_profiler::new_with_tracy_client` which will automatically report profiling data to tracy.

For details check the example code.

## Puffin integration

If you want to use [puffin](https://github.com/EmbarkStudios/puffin) for profiling, you can enable the `puffin` feature.

This adds `wgpu_profiler::puffin::output_frame_to_puffin` which makes it easy to report profiling data to a `puffin::GlobalProfiler`.

You can run the demo example with puffin by running `cargo run --example demo --features puffin`.
All CPU profiling goes to port `8585`, all GPU profiling goes to port `8586`. You can open puffin viewers for both.
```sh
puffin_viewer --url 127.0.0.1:8585
puffin_viewer --url 127.0.0.1:8586
```

For details check the example code.

# Internals

For every frame that hasn't completely finished processing yet
(i.e. hasn't returned results via [`GpuProfiler::process_finished_frame`])
we keep a `PendingFrame` around.

Whenever a profiling scope is opened, we allocate two queries.
This is done by either using the most recent `QueryPool` or creating a new one if there's no non-exhausted one ready.
Ideally, we only ever need a single `QueryPool` per frame! In order to converge to this,
we allocate new query pools with the size of all previous query pools in a given frame, effectively doubling the size.
On [`GpuProfiler::end_frame`], we memorize the total size of all `QueryPool`s in the current frame and make this the new minimum pool size.

`QueryPool` from finished frames are re-used, unless they are deemed too small.
*/

pub mod chrometrace;
mod errors;
mod profiler;
mod profiler_command_recorder;
mod profiler_query;
mod profiler_settings;
#[cfg(feature = "puffin")]
pub mod puffin;
mod scope;
#[cfg(feature = "tracy")]
mod tracy;

pub use errors::{CreationError, EndFrameError, SettingsError};
pub use profiler::GpuProfiler;
pub use profiler_command_recorder::ProfilerCommandRecorder;
pub use profiler_query::{GpuProfilerQuery, GpuTimerQueryResult};
pub use profiler_settings::GpuProfilerSettings;
pub use scope::{ManualOwningScope, OwningScope, Scope};
