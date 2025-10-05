use crate::CreationError;

pub fn create_tracy_gpu_client(
    backend: wgpu::Backend,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> Result<tracy_client::GpuContext, CreationError> {
    let query_set = device.create_query_set(&wgpu::QuerySetDescriptor {
        label: Some("wgpu-profiler gpu -> cpu sync query_set"),
        ty: wgpu::QueryType::Timestamp,
        count: 1,
    });

    let resolve_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("wgpu-profiler gpu -> cpu resolve buffer"),
        size: wgpu::QUERY_SIZE as _,
        usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });

    let map_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("wgpu-profiler gpu -> cpu map buffer"),
        size: wgpu::QUERY_SIZE as _,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let mut timestamp_encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("wgpu-profiler gpu -> cpu query timestamp"),
    });
    timestamp_encoder.write_timestamp(&query_set, 0);
    timestamp_encoder.resolve_query_set(&query_set, 0..1, &resolve_buffer, 0);
    // Workaround for https://github.com/gfx-rs/wgpu/issues/6406
    // TODO when that bug is fixed, merge these encoders together again
    let mut copy_encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("wgpu-profiler gpu -> cpu copy timestamp"),
    });
    copy_encoder.copy_buffer_to_buffer(&resolve_buffer, 0, &map_buffer, 0, wgpu::QUERY_SIZE as u64);
    queue.submit([timestamp_encoder.finish(), copy_encoder.finish()]);

    map_buffer.slice(..).map_async(wgpu::MapMode::Read, |_| ());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();

    let view = map_buffer.slice(..).get_mapped_range();
    let timestamp: i64 = i64::from_le_bytes((*view).try_into().unwrap());

    let tracy_backend = match backend {
        wgpu::Backend::Noop | wgpu::Backend::Metal | wgpu::Backend::BrowserWebGpu => {
            tracy_client::GpuContextType::Invalid
        }
        wgpu::Backend::Vulkan => tracy_client::GpuContextType::Vulkan,
        wgpu::Backend::Dx12 => tracy_client::GpuContextType::Direct3D12,
        wgpu::Backend::Gl => tracy_client::GpuContextType::OpenGL,
    };

    tracy_client::Client::running()
        .ok_or(CreationError::TracyClientNotRunning)?
        .new_gpu_context(
            Some("wgpu"),
            tracy_backend,
            timestamp,
            queue.get_timestamp_period(),
        )
        .map_err(CreationError::from)
}
