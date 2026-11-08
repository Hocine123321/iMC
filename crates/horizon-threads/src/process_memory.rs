//! Read-only process memory diagnostics.
/// The process's memory counters.
#[cfg(windows)]
fn counters() -> Option<windows_sys::Win32::System::ProcessStatus::PROCESS_MEMORY_COUNTERS_EX2> {
    use windows_sys::Win32::System::ProcessStatus::{
        K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX2,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX2>() as u32;
    // SAFETY: all-zero is a valid value of this plain C struct, which starts with the fields of the
    // one the call is declared with, and the call writes at most `size` bytes into it; the current
    // process's pseudo-handle needs no closing.
    unsafe {
        let mut counters: PROCESS_MEMORY_COUNTERS_EX2 = std::mem::zeroed();
        counters.cb = size;
        (K32GetProcessMemoryInfo(GetCurrentProcess(), (&raw mut counters).cast(), size) != 0)
            .then_some(counters)
    }
}

/// The memory this process holds in RAM, in bytes.
#[cfg(windows)]
pub fn resident_bytes() -> Option<u64> {
    Some(counters()?.WorkingSetSize as u64)
}

/// The private memory this process holds in RAM, all it holds in RAM and the machine's RAM, in
/// bytes.
#[cfg(windows)]
pub fn usage() -> Option<[u64; 3]> {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let counters = counters()?;
    let resident = counters.WorkingSetSize as u64;
    let private = match counters.PrivateWorkingSetSize {
        // Windows before 10 22H2 leaves it empty: the private commit is the nearest it has.
        0 => (counters.PrivateUsage as u64).min(resident),
        bytes => bytes as u64,
    };
    // SAFETY: all-zero is a valid value of this plain C struct, its length is set as the call
    // requires, and the call only writes into it.
    let ram = unsafe {
        let mut status: MEMORYSTATUSEX = std::mem::zeroed();
        status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        (GlobalMemoryStatusEx(&mut status) != 0).then_some(status.ullTotalPhys)
    }?;
    Some([private, resident, ram])
}

/// [`usage`]'s counters: not read on this system.
#[cfg(not(windows))]
pub fn usage() -> Option<[u64; 3]> {
    None
}

/// The memory this process holds in RAM, in bytes.
#[cfg(any(target_os = "linux", target_os = "android"))]
pub fn resident_bytes() -> Option<u64> {
    let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
    let pages: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;
    // SAFETY: reads one system constant.
    let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    Some(pages * u64::try_from(page).ok()?)
}

/// `task_vm_info` (`<mach/task_info.h>`) up to `phys_footprint`, the fields of `TASK_VM_INFO_REV1`.
/// libc has no binding for it.
#[cfg(target_vendor = "apple")]
#[repr(C)]
struct TaskVmInfo {
    virtual_size: u64,
    region_count: i32,
    page_size: i32,
    resident_size: u64,
    resident_size_peak: u64,
    device: u64,
    device_peak: u64,
    internal: u64,
    internal_peak: u64,
    external: u64,
    external_peak: u64,
    reusable: u64,
    reusable_peak: u64,
    purgeable_volatile_pmap: u64,
    purgeable_volatile_resident: u64,
    purgeable_volatile_virtual: u64,
    compressed: u64,
    compressed_peak: u64,
    compressed_lifetime: u64,
    phys_footprint: u64,
}

#[cfg(target_vendor = "apple")]
unsafe extern "C" {
    /// This process's task port, which the `mach_task_self()` macro reads. libc declares it for
    /// macOS only; it is exported by iOS's libSystem too.
    static mach_task_self_: libc::mach_port_t;
}

/// This process's `task_vm_info`, from the Mach `task_info` call (the one iOS allows; its libproc
/// is not in the SDK).
#[cfg(target_vendor = "apple")]
fn task_vm_info() -> Option<TaskVmInfo> {
    const TASK_VM_INFO: libc::task_flavor_t = 22;
    // The kernel writes at most `count` 32-bit words, and `count` covers exactly this struct.
    let mut count = (std::mem::size_of::<TaskVmInfo>() / std::mem::size_of::<libc::integer_t>())
        as libc::mach_msg_type_number_t;
    // SAFETY: all-zero is a valid value of this plain C struct; `count` bounds what the call writes
    // into it; `mach_task_self_` is this process's own task port and needs no closing.
    unsafe {
        let mut info: TaskVmInfo = std::mem::zeroed();
        let status = libc::task_info(
            mach_task_self_,
            TASK_VM_INFO,
            (&raw mut info).cast(),
            &mut count,
        );
        (status == libc::KERN_SUCCESS).then_some(info)
    }
}

/// The physical footprint of this process, in bytes: what memory pressure is judged by on iOS
/// (it counts GPU allocations on unified memory), and what Xcode's memory gauge shows.
#[cfg(target_vendor = "apple")]
pub fn footprint_bytes() -> Option<u64> {
    Some(task_vm_info()?.phys_footprint)
}

/// The memory this process holds in RAM, in bytes. iOS has no libproc: the Mach `task_info`
/// resident size.
#[cfg(target_os = "ios")]
pub fn resident_bytes() -> Option<u64> {
    Some(task_vm_info()?.resident_size)
}

/// The memory this process holds in RAM, in bytes.
#[cfg(target_os = "macos")]
pub fn resident_bytes() -> Option<u64> {
    let size = std::mem::size_of::<libc::proc_taskinfo>() as libc::c_int;
    // SAFETY: all-zero is a valid value of this plain C struct, and the call writes at most `size`
    // bytes into it.
    unsafe {
        let mut info: libc::proc_taskinfo = std::mem::zeroed();
        let written = libc::proc_pidinfo(
            libc::getpid(),
            libc::PROC_PIDTASKINFO,
            0,
            (&raw mut info).cast(),
            size,
        );
        (written == size).then_some(info.pti_resident_size)
    }
}

/// The memory this process holds in RAM, in bytes: not known on this system.
#[cfg(not(any(
    windows,
    target_os = "linux",
    target_os = "android",
    target_vendor = "apple"
)))]
pub fn resident_bytes() -> Option<u64> {
    None
}

#[cfg(all(test, any(windows, target_os = "linux", target_vendor = "apple")))]
mod tests {
    use super::*;

    #[cfg(target_vendor = "apple")]
    #[test]
    fn the_footprint_is_reported() {
        assert!(footprint_bytes().unwrap() > 0);
    }

    #[test]
    fn touched_memory_counts_as_resident() {
        const SIZE: usize = 256 << 20;
        let before = resident_bytes().unwrap();
        let block = vec![1u8; SIZE];
        let during = resident_bytes().unwrap();
        std::hint::black_box(&block);
        assert!(
            during >= before + SIZE as u64 / 2,
            "{before} then {during} bytes"
        );
    }
}
