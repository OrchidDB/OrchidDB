//! Opt-in phase measurements. CPU deltas cover this native process (all
//! threads), not external JVM processes. Nested phases must not be added twice.
use std::sync::OnceLock;
use std::time::Instant;

static ENABLED: OnceLock<bool> = OnceLock::new();
pub(crate) struct Phase {
    name: &'static str,
    started: Option<(Instant, Option<f64>)>,
}
impl Phase {
    pub(crate) fn new(name: &'static str) -> Self {
        let enabled =
            *ENABLED.get_or_init(|| std::env::var_os("CRABGRAPH_PROFILE_PHASES").is_some());
        Self {
            name,
            started: enabled.then(|| (Instant::now(), cpu_ms())),
        }
    }
}
impl Drop for Phase {
    fn drop(&mut self) {
        if let Some((started, cpu)) = self.started {
            let wall_ms = started.elapsed().as_secs_f64() * 1000.0;
            let process_cpu_ms = cpu.zip(cpu_ms()).map(|(before, after)| after - before);
            eprintln!(
                "phase-profile {}",
                serde_json::json!({
                    "phase": self.name, "pid": std::process::id(),
                    "wall_ms": wall_ms, "native_process_cpu_ms": process_cpu_ms,
                })
            );
        }
    }
}
#[cfg(unix)]
fn cpu_ms() -> Option<f64> {
    // SAFETY: rusage is a C record of integer fields. A zeroed instance is
    // valid; getrusage writes it synchronously, and fields are read on success.
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) } != 0 {
        return None;
    }
    Some(
        (usage.ru_utime.tv_sec + usage.ru_stime.tv_sec) as f64 * 1000.0
            + (usage.ru_utime.tv_usec + usage.ru_stime.tv_usec) as f64 / 1000.0,
    )
}
#[cfg(not(unix))]
fn cpu_ms() -> Option<f64> {
    None
}
