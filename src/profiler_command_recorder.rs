/// Trait for exposing the methods of `wgpu::CommandEncoder`, `wgpu::RenderPass` and `wgpu::ComputePass` that are used by the profiler.
pub trait ProfilerCommandRecorder {
    /// Returns `true` if it's a pass or `false` if it's an encoder
    fn is_pass(&self) -> bool;
    fn write_timestamp(&mut self, query_set: &wgpu::QuerySet, query_index: u32);
    fn push_debug_group(&mut self, label: &str);
    fn pop_debug_group(&mut self);
}

macro_rules! ImplProfilerCommandRecorder {
    ($($name:ident $(< $lt:lifetime >)? : $pass:literal,)*) => {
        $(
            impl $(< $lt >)? ProfilerCommandRecorder for wgpu::$name $(< $lt >)? {
                fn is_pass(&self) -> bool { $pass }

                fn write_timestamp(&mut self, query_set: &wgpu::QuerySet, query_index: u32) {
                    self.write_timestamp(query_set, query_index)
                }

                fn push_debug_group(&mut self, label: &str) {
                    self.push_debug_group(label)
                }

                fn pop_debug_group(&mut self) {
                    self.pop_debug_group()
                }
            }
        )*
    };
}

ImplProfilerCommandRecorder!(CommandEncoder:false, RenderPass<'a>:true, ComputePass<'a>:true,);
