use std::{fs::File, io::Write, path::Path};

use crate::GpuTimerScopeResult;

/// Writes a .json trace file that can be viewed as a flame graph in Chrome or Edge via <chrome://tracing>
pub fn write_chrometrace(target: &Path, profile_data: &[GpuTimerScopeResult]) -> std::io::Result<()> {
    let mut file = File::create(target)?;

    writeln!(file, "{{")?;
    writeln!(file, "\"traceEvents\": [")?;

    let mut it = profile_data.iter().peekable();
    while let Some(child) = it.next() {
        write_results_recursive(&mut file, child, it.peek().is_none())?;
    }

    writeln!(file, "]")?;
    writeln!(file, "}}")?;

    Ok(())
}

fn write_results_recursive(file: &mut File, result: &GpuTimerScopeResult, last: bool) -> std::io::Result<()> {
    // Ignore "unprofiled" scopes for now.
    // TODO: It would be nice if they'd still show up in the flam graph if they have subscopes!
    if let Some(time) = &result.time {
        // note: ThreadIds are under the control of Rust’s standard library
        // and there may not be any relationship between ThreadId and the underlying platform’s notion of a thread identifier
        //
        // There's a proposal for stabilization of ThreadId::as_u64, which
        // would eliminate the need for this hack: https://github.com/rust-lang/rust/pull/110738
        //
        // for now, we use this hack to convert to integer
        let tid_to_int = |tid| {
            format!("{:?}", tid)
                .replace("ThreadId(", "")
                .replace(')', "")
                .parse::<u64>()
                .unwrap_or(std::u64::MAX)
        };

        write!(
            file,
            r#"{{ "pid":{}, "tid":{}, "ts":{}, "dur":{}, "ph":"X", "name":"{}" }}{}"#,
            result.pid,
            tid_to_int(result.tid),
            time.start * 1000.0 * 1000.0,
            (time.end - time.start) * 1000.0 * 1000.0,
            result.label,
            if last && result.nested_scopes.is_empty() { "\n" } else { ",\n" }
        )?;
    }
    if !result.nested_scopes.is_empty() {
        for child in result.nested_scopes.iter().take(result.nested_scopes.len() - 1) {
            write_results_recursive(file, child, false)?;
        }
        write_results_recursive(file, result.nested_scopes.last().unwrap(), last)?;
    }

    Ok(())
    // { "pid":1, "tid":1, "ts":546867, "dur":121564, "ph":"X", "name":"DoThings"
}
