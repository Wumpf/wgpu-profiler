use std::{fs::File, io::Write, path::Path};

use crate::GpuTimerScopeResult;

/// Writes a .json trace file that can be viewed as a flame graph in Chrome via chrome://tracing
pub fn write_chrometrace(target: &Path, profile_data: &[GpuTimerScopeResult]) -> std::io::Result<()> {
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

fn write_results_recursive(file: &mut File, result: &GpuTimerScopeResult, last: bool) -> std::io::Result<()> {
    write!(
        file,
        r#"{{ "pid":1, "tid":1, "ts":{}, "dur":{}, "ph":"X", "name":"{}" }}{}"#,
        result.time.start * 1000.0 * 1000.0,
        (result.time.end - result.time.start) * 1000.0 * 1000.0,
        result.label,
        if last && result.nested_scopes.is_empty() { "\n" } else { ",\n" }
    )?;
    if result.nested_scopes.is_empty() {
        return Ok(());
    }

    for child in result.nested_scopes.iter().take(result.nested_scopes.len() - 1) {
        write_results_recursive(file, child, false)?;
    }
    write_results_recursive(file, result.nested_scopes.last().unwrap(), last)?;

    Ok(())
    // { "pid":1, "tid":1, "ts":546867, "dur":121564, "ph":"X", "name":"DoThings"
}
