//! Horloge commune : QueryPerformanceCounter en unités de 100 ns, la base de `SystemRelativeTime` (WGC) et de WASAPI.
use std::sync::OnceLock;
use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};

pub fn now_100ns() -> i64 {
    static FREQ: OnceLock<i128> = OnceLock::new();
    let freq = *FREQ.get_or_init(|| {
        let mut f = 0;
        unsafe { QueryPerformanceFrequency(&mut f).ok() };
        f.max(1) as i128
    });
    let mut c = 0;
    unsafe { QueryPerformanceCounter(&mut c).ok() };
    (c as i128 * 10_000_000 / freq) as i64
}
