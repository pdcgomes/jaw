//! What this process costs, as the kernel counts it: its memory now and at most, its CPU time, and
//! how often its threads woke; for the benches of jawd and of interfaces alike.

use std::time::Duration;

#[derive(Clone, Copy, Debug, Default)]
pub struct Usage {
    /// Memory as Activity Monitor counts it, the physical footprint; on Linux, the resident set.
    pub footprint: u64,
    pub resident: u64,
    /// The most `footprint` has been since the process started.
    pub peak: u64,
    pub cpu: Duration,
    /// Times its threads were woken: by timers' interrupts and from the CPU's idle on macOS, by
    /// giving up the CPU to wait on Linux.
    pub wakeups: u64,
    pub switches: u64,
}

fn cpu() -> Duration {
    // SAFETY: getrusage fills the struct it's given, which is ours and zeroed.
    let usage = unsafe {
        let mut usage: libc::rusage = std::mem::zeroed();
        libc::getrusage(libc::RUSAGE_SELF, &mut usage);
        usage
    };
    let time = |t: libc::timeval| Duration::from_secs(t.tv_sec as u64) + Duration::from_micros(t.tv_usec as u64);
    time(usage.ru_utime) + time(usage.ru_stime)
}

/// The task's own counts, which mach gives any process for itself, even where a sandbox refuses
/// proc_pid_rusage.
#[cfg(target_os = "macos")]
mod mach {
    unsafe extern "C" {
        /// This process's task, which libSystem sets before main.
        pub static mach_task_self_: u32;
        pub fn task_info(task: u32, flavor: u32, info: *mut i32, count: *mut u32) -> i32;
    }

    pub const TASK_EVENTS_INFO: u32 = 2;
    pub const TASK_POWER_INFO: u32 = 21;
    pub const TASK_VM_INFO: u32 = 22;

    #[repr(C)]
    #[derive(Default)]
    pub struct Events {
        pub faults: i32,
        pub pageins: i32,
        pub cow_faults: i32,
        pub messages_sent: i32,
        pub messages_received: i32,
        pub syscalls_mach: i32,
        pub syscalls_unix: i32,
        pub csw: i32,
    }

    #[repr(C)]
    #[derive(Default)]
    pub struct Power {
        pub total_user: u64,
        pub total_system: u64,
        pub interrupt_wakeups: u64,
        pub platform_idle_wakeups: u64,
        pub timer_wakeups_bin_1: u64,
        pub timer_wakeups_bin_2: u64,
    }

    /// task_vm_info, as far as its seventh revision; the kernel fills the third's footprint peak
    /// only when there's room for all of that revision.
    #[repr(C)]
    #[derive(Default)]
    pub struct Vm {
        pub virtual_size: u64,
        pub region_count: i32,
        pub page_size: i32,
        pub resident_size: u64,
        pub resident_size_peak: u64,
        /// From device to compressed_lifetime.
        pub between: [u64; 14],
        pub phys_footprint: u64,
        pub min_address: u64,
        pub max_address: u64,
        pub ledger_phys_footprint_peak: i64,
        /// The rest of the ledgers, and the later revisions' fields.
        pub later: [i64; 26],
    }

    pub fn info<T: Default>(flavor: u32) -> T {
        let mut info = T::default();
        let mut count = (std::mem::size_of::<T>() / 4) as u32;
        // SAFETY: task_info writes at most `count` 32-bit words into `info`, which holds that many,
        // about this process's own task.
        unsafe { task_info(mach_task_self_, flavor, &mut info as *mut T as *mut i32, &mut count) };
        info
    }
}

#[cfg(target_os = "macos")]
pub fn now() -> Usage {
    let vm: mach::Vm = mach::info(mach::TASK_VM_INFO);
    let power: mach::Power = mach::info(mach::TASK_POWER_INFO);
    let events: mach::Events = mach::info(mach::TASK_EVENTS_INFO);
    Usage {
        footprint: vm.phys_footprint,
        resident: vm.resident_size,
        peak: vm.ledger_phys_footprint_peak.max(0) as u64,
        cpu: cpu(),
        wakeups: power.interrupt_wakeups + power.platform_idle_wakeups,
        switches: events.csw.max(0) as u64,
    }
}

#[cfg(not(target_os = "macos"))]
pub fn now() -> Usage {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let kib = |field: &str| {
        status
            .lines()
            .find_map(|line| line.strip_prefix(field))
            .and_then(|rest| rest.split_whitespace().next()?.parse::<u64>().ok())
            .unwrap_or_default()
            * 1024
    };
    // SAFETY: getrusage fills the struct it's given, which is ours and zeroed.
    let usage = unsafe {
        let mut usage: libc::rusage = std::mem::zeroed();
        libc::getrusage(libc::RUSAGE_SELF, &mut usage);
        usage
    };
    let resident = kib("VmRSS:");
    Usage {
        footprint: resident,
        resident,
        peak: kib("VmHWM:"),
        cpu: cpu(),
        wakeups: usage.ru_nvcsw.max(0) as u64,
        switches: (usage.ru_nvcsw + usage.ru_nivcsw).max(0) as u64,
    }
}

/// How long ago this process started, as the kernel says, for timing what comes after a launch.
#[cfg(target_os = "macos")]
pub fn age() -> Option<Duration> {
    // SAFETY: proc_pidinfo fills at most the size it's told of the proc_bsdinfo it's given, which
    // is ours and zeroed.
    let info = unsafe {
        let mut info: libc::proc_bsdinfo = std::mem::zeroed();
        let size = std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
        let pid = std::process::id() as libc::c_int;
        if libc::proc_pidinfo(pid, libc::PROC_PIDTBSDINFO, 0, &mut info as *mut _ as *mut libc::c_void, size) != size {
            return None;
        }
        info
    };
    let started = std::time::UNIX_EPOCH
        + Duration::from_secs(info.pbi_start_tvsec)
        + Duration::from_micros(info.pbi_start_tvusec);
    std::time::SystemTime::now().duration_since(started).ok()
}

/// How long ago this process started, as the kernel says, to its clock's tick.
#[cfg(not(target_os = "macos"))]
pub fn age() -> Option<Duration> {
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    // The command's name is in parentheses and may hold spaces; the start is the 20th field after.
    let ticks: u64 = stat.rsplit_once(')')?.1.split_whitespace().nth(19)?.parse().ok()?;
    let uptime: f64 = std::fs::read_to_string("/proc/uptime").ok()?.split_whitespace().next()?.parse().ok()?;
    // SAFETY: sysconf reads a constant of the system.
    let hertz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) }.max(1) as f64;
    Some(Duration::from_secs_f64((uptime - ticks as f64 / hertz).max(0.0)))
}

/// The calling thread's CPU time so far.
pub fn thread_cpu() -> Duration {
    let mut now = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: clock_gettime fills the timespec it's given.
    unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut now) };
    Duration::new(now.tv_sec as u64, now.tv_nsec as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_process_has_memory_and_its_sleeps_wake_it() {
        let before = now();
        for _ in 0..20 {
            std::thread::sleep(Duration::from_millis(5));
        }
        let after = now();
        assert!(after.footprint > 0 && after.resident > 0, "{after:?}");
        assert!(after.peak >= after.footprint / 2, "{after:?}");
        assert!(after.wakeups > before.wakeups, "20 sleeps and no wake-ups: {before:?} then {after:?}");
    }

    #[test]
    fn a_process_knows_how_long_ago_it_started() {
        let age = age().expect("the kernel says when this process started");
        assert!(age > Duration::ZERO && age < Duration::from_secs(3600), "{age:?}");
    }
}
