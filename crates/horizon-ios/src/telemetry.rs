//! Telemetry for the iOS app (milestone M3): what the OS counts against the process, the frame
//! times, the thermal state. A sampler thread writes `Documents/telemetry/latest.json` (the report)
//! and `samples.csv`; the app declares file sharing, so both are readable from the Files app.
//!
//! The figures are measurements to read, not limits: nothing here assumes how much memory an iPhone
//! allows. The footprint is `phys_footprint` (what Xcode's gauge shows and memory pressure acts on),
//! `available` is `os_proc_available_memory()` where the OS offers it.

use std::{
    collections::VecDeque,
    fs,
    io::Write,
    path::PathBuf,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

const FRAME_WINDOW: usize = 300;
const SAMPLE_EVERY: Duration = Duration::from_millis(250);

/// One reading, and the running extremes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub uptime_s: f32,
    pub footprint_mb: f64,
    pub peak_footprint_mb: f64,
    pub resident_mb: f64,
    /// `os_proc_available_memory()`; 0 where the OS has no such call.
    pub available_mb: f64,
    pub min_available_mb: f64,
    /// 0 nominal, 1 fair, 2 serious, 3 critical.
    pub thermal: u8,
    pub peak_thermal: u8,
    pub low_power: bool,
    pub fps: f32,
    pub frame_p50_ms: f32,
    pub frame_p99_ms: f32,
    pub frames: u64,
}

pub struct Telemetry {
    mode: &'static str,
    started: Instant,
    frames: AtomicU64,
    frame_ms: Mutex<VecDeque<f32>>,
    latest: Mutex<Snapshot>,
    dir: PathBuf,
}

static TELEMETRY: OnceLock<Telemetry> = OnceLock::new();

fn mb(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

/// The OS's physical footprint of this process (Apple), else the resident size.
fn footprint() -> Option<u64> {
    #[cfg(target_vendor = "apple")]
    {
        horizon_threads::process_memory::footprint_bytes()
    }
    #[cfg(not(target_vendor = "apple"))]
    {
        horizon_threads::process_memory::resident_bytes()
    }
}

#[cfg(target_os = "ios")]
fn available() -> Option<u64> {
    unsafe extern "C" {
        // <os/proc.h>, iOS 13 and later.
        fn os_proc_available_memory() -> usize;
    }
    // SAFETY: a plain query with no arguments and no side effects.
    let bytes = unsafe { os_proc_available_memory() };
    (bytes > 0).then_some(bytes as u64)
}
#[cfg(not(target_os = "ios"))]
fn available() -> Option<u64> {
    None
}

/// (thermal state 0 to 3, low power mode) from `NSProcessInfo`.
#[cfg(target_os = "ios")]
fn thermal() -> (u8, bool) {
    use objc2::{
        msg_send,
        runtime::{AnyClass, AnyObject},
    };
    let Some(class) = AnyClass::get(c"NSProcessInfo") else {
        return (0, false);
    };
    // SAFETY: `processInfo` is the shared instance; `thermalState` returns an NSInteger enum and
    // `isLowPowerModeEnabled` a BOOL. Both are documented thread-safe properties.
    unsafe {
        let info: *mut AnyObject = msg_send![class, processInfo];
        if info.is_null() {
            return (0, false);
        }
        let state: isize = msg_send![info, thermalState];
        let low_power: bool = msg_send![info, isLowPowerModeEnabled];
        (state.clamp(0, 3) as u8, low_power)
    }
}
#[cfg(not(target_os = "ios"))]
fn thermal() -> (u8, bool) {
    (0, false)
}

pub fn thermal_name(state: u8) -> &'static str {
    match state {
        0 => "NOMINAL",
        1 => "FAIR",
        2 => "SERIOUS",
        _ => "CRITICAL",
    }
}

/// Where the reports go: the app's Documents folder on iOS, the temp folder elsewhere.
fn report_dir() -> PathBuf {
    #[cfg(target_os = "ios")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join("Documents").join("telemetry");
        }
    }
    std::env::temp_dir().join("horizon-ios-telemetry")
}

/// The value at fraction `q` (0 to 1) of an ascending-sorted list.
fn percentile(sorted: &[f32], q: f32) -> f32 {
    if sorted.is_empty() {
        return 0.0;
    }
    let index = ((sorted.len() - 1) as f32 * q).round() as usize;
    sorted[index.min(sorted.len() - 1)]
}

impl Telemetry {
    fn new(mode: &'static str) -> Self {
        Self {
            mode,
            started: Instant::now(),
            frames: AtomicU64::new(0),
            frame_ms: Mutex::new(VecDeque::with_capacity(FRAME_WINDOW)),
            latest: Mutex::new(Snapshot::default()),
            dir: report_dir(),
        }
    }

    fn sample(&self, previous_frames: &mut u64, previous_at: &mut Instant) -> Snapshot {
        let now = Instant::now();
        let frames = self.frames.load(Ordering::Relaxed);
        let seconds = now.duration_since(*previous_at).as_secs_f32().max(1e-3);
        let fps = (frames - *previous_frames) as f32 / seconds;
        *previous_frames = frames;
        *previous_at = now;

        let (p50, p99) = {
            let window = self.frame_ms.lock().unwrap_or_else(|e| e.into_inner());
            let mut sorted: Vec<f32> = window.iter().copied().collect();
            sorted.sort_by(|a, b| a.total_cmp(b));
            (percentile(&sorted, 0.5), percentile(&sorted, 0.99))
        };
        let footprint_mb = footprint().map(mb).unwrap_or(0.0);
        let resident_mb = horizon_threads::process_memory::resident_bytes()
            .map(mb)
            .unwrap_or(0.0);
        let available_mb = available().map(mb).unwrap_or(0.0);
        let (thermal, low_power) = thermal();

        let mut latest = self.latest.lock().unwrap_or_else(|e| e.into_inner());
        let snapshot = Snapshot {
            uptime_s: self.started.elapsed().as_secs_f32(),
            footprint_mb,
            peak_footprint_mb: latest.peak_footprint_mb.max(footprint_mb),
            resident_mb,
            available_mb,
            min_available_mb: match (available_mb > 0.0, latest.min_available_mb > 0.0) {
                (false, _) => 0.0,
                (true, false) => available_mb,
                (true, true) => latest.min_available_mb.min(available_mb),
            },
            thermal,
            peak_thermal: latest.peak_thermal.max(thermal),
            low_power,
            fps,
            frame_p50_ms: p50,
            frame_p99_ms: p99,
            frames,
        };
        *latest = snapshot.clone();
        snapshot
    }

    fn json(&self, s: &Snapshot) -> String {
        format!(
            concat!(
                "{{\n",
                "  \"app\": \"horizon-ios\",\n",
                "  \"mode\": \"{}\",\n",
                "  \"os\": \"{}\",\n",
                "  \"uptime_s\": {:.1},\n",
                "  \"footprint_mb\": {:.1},\n",
                "  \"peak_footprint_mb\": {:.1},\n",
                "  \"resident_mb\": {:.1},\n",
                "  \"available_mb\": {:.1},\n",
                "  \"min_available_mb\": {:.1},\n",
                "  \"thermal\": \"{}\",\n",
                "  \"peak_thermal\": \"{}\",\n",
                "  \"low_power_mode\": {},\n",
                "  \"fps\": {:.1},\n",
                "  \"frame_p50_ms\": {:.2},\n",
                "  \"frame_p99_ms\": {:.2},\n",
                "  \"frames\": {}\n",
                "}}\n"
            ),
            self.mode,
            std::env::consts::OS,
            s.uptime_s,
            s.footprint_mb,
            s.peak_footprint_mb,
            s.resident_mb,
            s.available_mb,
            s.min_available_mb,
            thermal_name(s.thermal),
            thermal_name(s.peak_thermal),
            s.low_power,
            s.fps,
            s.frame_p50_ms,
            s.frame_p99_ms,
            s.frames
        )
    }

    fn write_report(&self, s: &Snapshot) {
        let tmp = self.dir.join("latest.json.tmp");
        if fs::write(&tmp, self.json(s)).is_ok() {
            let _ = fs::rename(&tmp, self.dir.join("latest.json"));
        }
    }
}

/// Starts the sampler thread (once). `mode` names what the app is running ("cube", "game").
pub fn start(mode: &'static str) {
    if TELEMETRY.set(Telemetry::new(mode)).is_err() {
        return;
    }
    let Some(telemetry) = TELEMETRY.get() else {
        return;
    };
    let _ = fs::create_dir_all(&telemetry.dir);
    eprintln!("horizon-ios: telemetry in {}", telemetry.dir.display());
    let spawned = std::thread::Builder::new()
        .name("telemetry".into())
        .spawn(move || {
            let mut csv = fs::File::create(telemetry.dir.join("samples.csv")).ok();
            if let Some(file) = csv.as_mut() {
                let _ = writeln!(
                    file,
                    "uptime_s,footprint_mb,resident_mb,available_mb,thermal,low_power,fps,frame_p50_ms,frame_p99_ms"
                );
            }
            let mut previous_frames = 0;
            let mut previous_at = Instant::now();
            let mut tick: u64 = 0;
            loop {
                std::thread::sleep(SAMPLE_EVERY);
                tick += 1;
                let s = telemetry.sample(&mut previous_frames, &mut previous_at);
                if tick % 4 == 0 {
                    if let Some(file) = csv.as_mut() {
                        let _ = writeln!(
                            file,
                            "{:.1},{:.1},{:.1},{:.1},{},{},{:.1},{:.2},{:.2}",
                            s.uptime_s,
                            s.footprint_mb,
                            s.resident_mb,
                            s.available_mb,
                            s.thermal,
                            s.low_power as u8,
                            s.fps,
                            s.frame_p50_ms,
                            s.frame_p99_ms
                        );
                    }
                }
                if tick % 20 == 0 {
                    telemetry.write_report(&s);
                    eprintln!(
                        "horizon-ios: footprint {:.1} MB (peak {:.1}), available {:.1} MB, {} , {:.1} fps",
                        s.footprint_mb,
                        s.peak_footprint_mb,
                        s.available_mb,
                        thermal_name(s.thermal),
                        s.fps
                    );
                }
            }
        });
    if let Err(e) = spawned {
        eprintln!("horizon-ios: telemetry thread failed: {e}");
    }
}

/// Records one frame's time (the render thread calls this once per frame).
pub fn frame(ms: f32) {
    let Some(telemetry) = TELEMETRY.get() else {
        return;
    };
    telemetry.frames.fetch_add(1, Ordering::Relaxed);
    let mut window = telemetry.frame_ms.lock().unwrap_or_else(|e| e.into_inner());
    if window.len() == FRAME_WINDOW {
        window.pop_front();
    }
    window.push_back(ms);
}

/// The overlay's text: upper case, because the overlay's font is.
pub fn overlay_lines() -> Vec<String> {
    let Some(telemetry) = TELEMETRY.get() else {
        return vec!["TELEMETRY OFF".into()];
    };
    let s = telemetry
        .latest
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    vec![
        format!(
            "FPS {:.1}  P50 {:.1}  P99 {:.1} MS",
            s.fps, s.frame_p50_ms, s.frame_p99_ms
        ),
        format!(
            "FOOTPRINT {:.1} MB  PEAK {:.1}",
            s.footprint_mb, s.peak_footprint_mb
        ),
        format!(
            "AVAILABLE {:.0} MB  MIN {:.0}",
            s.available_mb, s.min_available_mb
        ),
        format!(
            "THERMAL {}  LOW POWER {}",
            thermal_name(s.thermal),
            if s.low_power { "ON" } else { "OFF" }
        ),
        format!("UPTIME {:.0} S  MODE {}", s.uptime_s, telemetry.mode.to_uppercase()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_pick_ranks() {
        let v: Vec<f32> = (1..=100).map(|n| n as f32).collect();
        assert_eq!(percentile(&v, 0.0), 1.0);
        assert_eq!(percentile(&v, 1.0), 100.0);
        assert!((percentile(&v, 0.5) - 50.0).abs() <= 1.0);
        assert_eq!(percentile(&[], 0.5), 0.0);
    }

    #[test]
    fn the_report_is_json_with_every_field() {
        let t = Telemetry::new("test");
        let s = Snapshot {
            footprint_mb: 12.5,
            thermal: 2,
            peak_thermal: 3,
            ..Snapshot::default()
        };
        let json = t.json(&s);
        for key in [
            "\"mode\": \"test\"",
            "\"footprint_mb\": 12.5",
            "\"thermal\": \"SERIOUS\"",
            "\"peak_thermal\": \"CRITICAL\"",
            "\"frames\": 0",
        ] {
            assert!(json.contains(key), "{key} missing from {json}");
        }
        assert!(json.trim_end().ends_with('}'));
    }

    #[test]
    fn thermal_names_cover_the_four_states() {
        assert_eq!(thermal_name(0), "NOMINAL");
        assert_eq!(thermal_name(1), "FAIR");
        assert_eq!(thermal_name(2), "SERIOUS");
        assert_eq!(thermal_name(3), "CRITICAL");
        assert_eq!(thermal_name(9), "CRITICAL");
    }

    #[test]
    fn a_sample_counts_frames_and_keeps_extremes() {
        let t = Telemetry::new("test");
        let mut frames = 0;
        let mut at = Instant::now() - Duration::from_secs(1);
        t.frames.store(60, Ordering::Relaxed);
        {
            let mut w = t.frame_ms.lock().unwrap();
            w.extend([16.0, 17.0, 40.0]);
        }
        let s = t.sample(&mut frames, &mut at);
        assert!(s.fps > 40.0 && s.fps < 80.0, "fps {}", s.fps);
        assert_eq!(s.frames, 60);
        assert_eq!(s.frame_p99_ms, 40.0);
        assert!(s.peak_footprint_mb >= s.footprint_mb);
    }
}
