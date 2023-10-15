/// Easy to use profiling scope.
///
/// Example:
/// ```ignore
/// wgpu_profiler!("name of your scope", &mut profiler, &mut encoder, &device, {
///     // wgpu commands go here
/// })
/// ```
#[macro_export]
macro_rules! wgpu_profiler {
    ($label:expr, $profiler:expr, $encoder_or_pass:expr, $device:expr, $code:expr) => {{
        let $profiler.begin_scope($label, $encoder_or_pass, $device);
        let ret = $code;
        $profiler.end_scope($encoder_or_pass).unwrap();
        ret
    }};
}
