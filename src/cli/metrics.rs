use std::time::Duration;

pub struct IndexMetrics {
    pub scan_duration: Duration,
    pub parse_duration: Duration,
    pub resolve_duration: Duration,
    pub community_duration: Duration,
    pub process_duration: Duration,
    pub store_duration: Duration,
    pub total_duration: Duration,
    pub files_scanned: usize,
    pub files_parsed: usize,
    pub parse_failures: usize,
    pub symbol_count: usize,
    pub relationship_count: usize,
    pub community_count: usize,
    pub process_count: usize,
    pub peak_rss_bytes: Option<u64>,
}

impl IndexMetrics {
    pub fn print(&self) {
        eprintln!("\n--- Index Metrics ---");
        eprintln!("scan_duration:      {:.3}s", self.scan_duration.as_secs_f64());
        eprintln!("parse_duration:     {:.3}s", self.parse_duration.as_secs_f64());
        eprintln!("resolve_duration:   {:.3}s", self.resolve_duration.as_secs_f64());
        eprintln!("community_duration: {:.3}s", self.community_duration.as_secs_f64());
        eprintln!("process_duration:   {:.3}s", self.process_duration.as_secs_f64());
        eprintln!("store_duration:     {:.3}s", self.store_duration.as_secs_f64());
        eprintln!("total_duration:     {:.3}s", self.total_duration.as_secs_f64());
        eprintln!("files_scanned:      {}", self.files_scanned);
        eprintln!("files_parsed:       {}", self.files_parsed);
        eprintln!("parse_failures:     {}", self.parse_failures);
        eprintln!("symbol_count:       {}", self.symbol_count);
        eprintln!("relationship_count: {}", self.relationship_count);
        eprintln!("community_count:    {}", self.community_count);
        eprintln!("process_count:      {}", self.process_count);
        if let Some(rss) = self.peak_rss_bytes {
            eprintln!("peak_rss_bytes:     {} ({:.1} MB)", rss, rss as f64 / 1_048_576.0);
        }
    }
}

/// Return peak RSS (max resident set size) in bytes.
/// Uses getrusage(RUSAGE_SELF).ru_maxrss on macOS and Linux.
/// macOS returns bytes; Linux returns kilobytes (converted here).
pub fn peak_rss_bytes() -> Option<u64> {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        use std::mem;
        unsafe {
            let mut usage: libc::rusage = mem::zeroed();
            if libc::getrusage(libc::RUSAGE_SELF, &mut usage) == 0 {
                let rss = usage.ru_maxrss as u64;
                #[cfg(target_os = "macos")]
                {
                    Some(rss) // macOS: already in bytes
                }
                #[cfg(target_os = "linux")]
                {
                    Some(rss * 1024) // Linux: in kilobytes
                }
            } else {
                None
            }
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        None
    }
}
