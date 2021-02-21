macro_rules! wgpu_scope {
    ($label:expr, $profiler:expr, $encoder_or_pass:expr, $device:expr, $code:expr) => {{
        $profiler.begin_scope($label, $encoder_or_pass, $device);
        let ret = $code;
        $profiler.end_scope($encoder_or_pass);
        ret
    }};
}
