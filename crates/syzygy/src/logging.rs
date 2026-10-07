//! File logging, set up before anything else runs.

use flexi_logger::{
    Cleanup, Criterion, Duplicate, FileSpec, Logger, LoggerHandle, Naming, WriteMode,
};
use std::path::Path;

use crate::identity::APP_NAME;

/// syzygy's crates at `debug`, dependencies at `info` (zbus, which logs every
/// D-Bus handshake, at `warn`). `RUST_LOG` overrides it.
const DEFAULT_SPEC: &str = "syzygy=debug,syzygy_store=debug,zbus=warn,info";

/// Initialize the global logger. Must be called exactly once at startup,
/// before any `log::*!` macros are invoked.
///
/// Writes to `<log_dir>/syzygy_rCURRENT.log` (rotated) and duplicates output
/// to stderr. Falls back to stderr-only on any file-system error.
///
/// The caller MUST keep the returned handle alive for the process lifetime:
/// dropping it stops the background log writer.
pub fn init(log_dir: &Path) -> LoggerHandle {
    if let Err(e) = std::fs::create_dir_all(log_dir) {
        eprintln!(
            "{APP_NAME}: could not create log directory {} ({e}), falling back to stderr-only logging",
            log_dir.display(),
        );
        return stderr_only();
    }

    match base()
        .log_to_file(FileSpec::default().directory(log_dir).basename(APP_NAME))
        .duplicate_to_stderr(Duplicate::All)
        .rotate(
            Criterion::Size(5_000_000), // 5 MB
            Naming::Numbers,
            Cleanup::KeepLogFiles(9), // 9 rotated + 1 active = ~50 MB max
        )
        .format_for_files(flexi_logger::detailed_format)
        .write_mode(WriteMode::BufferAndFlush)
        .start()
    {
        Ok(handle) => handle,
        Err(e) => {
            eprintln!(
                "{APP_NAME}: file logger init failed ({e}), falling back to stderr-only logging"
            );
            stderr_only()
        }
    }
}

fn base() -> Logger {
    Logger::try_with_env_or_str(DEFAULT_SPEC)
        .expect("flexi_logger spec parsing should never fail for a static spec")
}

fn stderr_only() -> LoggerHandle {
    base()
        .log_to_stderr()
        .write_mode(WriteMode::BufferAndFlush)
        .start()
        .expect("stderr logger should always start")
}
