use std::{fs::File, io::Write, path::Path};

use crate::GpuTimerQueryResult;

/// Writes a .json trace file that can be viewed as a flame graph in Chrome or Edge via <chrome://tracing>
pub fn write_chrometrace(
    target: &Path,
    profile_data: &[GpuTimerQueryResult],
) -> std::io::Result<()> {
    let mut file = File::create(target)?;

    writeln!(file, "{{")?;
    writeln!(file, "\"traceEvents\": [")?;

    if !profile_data.is_empty() {
        for child in profile_data.iter().take(profile_data.len() - 1) {
            write_results_recursive(&mut file, child, false)?;
        }
        write_results_recursive(&mut file, profile_data.last().unwrap(), true)?;
    }

    writeln!(file, "]")?;
    writeln!(file, "}}")?;

    Ok(())
}

fn write_results_recursive(
    file: &mut File,
    result: &GpuTimerQueryResult,
    last: bool,
) -> std::io::Result<()> {
    let GpuTimerQueryResult {
        label,
        pid,
        tid,
        time,
        nested_queries,
    } = result;

    if let Some(time) = time {
        // note: ThreadIds are under the control of Rust’s standard library
        // and there may not be any relationship between ThreadId and the underlying platform’s notion of a thread identifier
        //
        // There's a proposal for stabilization of ThreadId::as_u64, which
        // would eliminate the need for this hack: https://github.com/rust-lang/rust/pull/110738
        //
        // for now, we use this hack to convert to integer
        let tid_to_int = |tid| {
            format!("{tid:?}")
                .replace("ThreadId(", "")
                .replace(')', "")
                .parse::<u64>()
                .unwrap_or(u64::MAX)
        };
        write!(
            file,
            r#"{{ "pid":{}, "tid":{}, "ts":{}, "dur":{}, "ph":"X", "name":"{}" }}{}"#,
            pid,
            tid_to_int(tid),
            time.start * 1000.0 * 1000.0,
            (time.end - time.start) * 1000.0 * 1000.0,
            label,
            if last && nested_queries.is_empty() {
                "\n"
            } else {
                ",\n"
            }
        )?;
    }
    if nested_queries.is_empty() {
        return Ok(());
    }

    for child in nested_queries.iter().take(nested_queries.len() - 1) {
        write_results_recursive(file, child, false)?;
    }
    write_results_recursive(file, nested_queries.last().unwrap(), last)?;

    Ok(())
    // { "pid":1, "tid":1, "ts":546867, "dur":121564, "ph":"X", "name":"DoThings"
}
