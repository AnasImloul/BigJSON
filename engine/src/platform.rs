//! Platform compatibility shims for the few OS-specific primitives the
//! engine relies on: positional (offset-addressed) file I/O, madvise-style
//! memory-map hints, and process-liveness probing. Unix delegates to the
//! native `pread`/`pwrite`/`madvise`/`kill` facilities; Windows uses the
//! equivalent Win32 calls (with madvise hints degrading to no-ops).

use memmap2::Mmap;
use std::fs::File;
use std::io;

/// Positional file I/O with exact/all semantics, independent of (and
/// without disturbing) the file's seek position on Unix.
///
/// On Unix this is `pread`/`pwrite` via `std::os::unix::fs::FileExt`,
/// which are atomic w.r.t. the offset and leave the cursor untouched.
/// On Windows `seek_read`/`seek_write` can transfer fewer bytes than
/// requested and *do* move the cursor, so we loop until the whole
/// buffer is satisfied. Callers must not issue concurrent positional
/// I/O against the same handle from multiple threads on Windows; the
/// engine's sinks each own a private `File`, so this holds.
pub trait PositionalIo {
    fn read_exact_at(&self, buf: &mut [u8], offset: u64) -> io::Result<()>;
    fn write_all_at(&self, buf: &[u8], offset: u64) -> io::Result<()>;
}

#[cfg(unix)]
impl PositionalIo for File {
    #[inline]
    fn read_exact_at(&self, buf: &mut [u8], offset: u64) -> io::Result<()> {
        std::os::unix::fs::FileExt::read_exact_at(self, buf, offset)
    }

    #[inline]
    fn write_all_at(&self, buf: &[u8], offset: u64) -> io::Result<()> {
        std::os::unix::fs::FileExt::write_all_at(self, buf, offset)
    }
}

#[cfg(windows)]
impl PositionalIo for File {
    fn read_exact_at(&self, mut buf: &mut [u8], mut offset: u64) -> io::Result<()> {
        use std::os::windows::fs::FileExt;
        while !buf.is_empty() {
            match self.seek_read(buf, offset) {
                Ok(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "failed to fill whole buffer",
                    ))
                }
                Ok(n) => {
                    buf = &mut buf[n..];
                    offset += n as u64;
                }
                Err(ref e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    fn write_all_at(&self, mut buf: &[u8], mut offset: u64) -> io::Result<()> {
        use std::os::windows::fs::FileExt;
        while !buf.is_empty() {
            match self.seek_write(buf, offset) {
                Ok(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "failed to write whole buffer",
                    ))
                }
                Ok(n) => {
                    buf = &buf[n..];
                    offset += n as u64;
                }
                Err(ref e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
}

/// Best-effort paging hints for a memory map. Hints never affect
/// correctness, only the kernel's prefetch/eviction heuristics, so the
/// Windows implementations are no-ops (the Win32 `PrefetchVirtualMemory`
/// / `OfferVirtualMemory` equivalents aren't worth the FFI surface here).
pub trait MmapHints {
    /// Hint that access will be random (disables read-ahead). Whole map.
    fn hint_random(&self);
    /// Hint that `[offset, offset+len)` will be needed soon (prefetch).
    fn hint_will_need(&self, offset: usize, len: usize);
    /// Hint that `[offset, offset+len)` is no longer needed and its
    /// pages may be dropped from the resident set.
    ///
    /// # Safety
    /// On Unix this maps to `MADV_DONTNEED`, which for a private mapping
    /// would discard modifications. The engine only calls this on a
    /// read-only shared file map where dropping clean pages is harmless,
    /// but the contract is still `unsafe` to mirror memmap2's API.
    unsafe fn hint_dont_need(&self, offset: usize, len: usize);
}

#[cfg(unix)]
impl MmapHints for Mmap {
    #[inline]
    fn hint_random(&self) {
        let _ = self.advise(memmap2::Advice::Random);
    }

    #[inline]
    fn hint_will_need(&self, offset: usize, len: usize) {
        let _ = self.advise_range(memmap2::Advice::WillNeed, offset, len);
    }

    #[inline]
    unsafe fn hint_dont_need(&self, offset: usize, len: usize) {
        let _ = self.unchecked_advise_range(memmap2::UncheckedAdvice::DontNeed, offset, len);
    }
}

#[cfg(windows)]
impl MmapHints for Mmap {
    #[inline]
    fn hint_random(&self) {}

    #[inline]
    fn hint_will_need(&self, _offset: usize, _len: usize) {}

    #[inline]
    unsafe fn hint_dont_need(&self, _offset: usize, _len: usize) {}
}

/// Checks whether a process exists, without perturbing it. Used only to
/// garbage-collect stale `.streaming.tmp` sidecars left behind by a
/// crashed run, so a false "alive" merely leaks a tmp file until the
/// next sweep — never data loss.
#[cfg(unix)]
pub fn pid_is_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    // SAFETY: `kill` with sig=0 just probes existence/permission; it
    // delivers no signal.
    let r = unsafe { libc::kill(pid as libc::pid_t, 0) };
    if r == 0 {
        return true;
    }
    match io::Error::last_os_error().raw_os_error() {
        // Exists but we can't signal it — treat as alive to be safe.
        Some(libc::EPERM) => true,
        // ESRCH or anything else: treat as gone.
        _ => false,
    }
}

/// Windows liveness probe via `OpenProcess` + `GetExitCodeProcess`.
/// The Win32 symbols are declared inline (linked from `kernel32`, which
/// is in the default Windows link set) to avoid pulling in a binding
/// crate just for three calls.
#[cfg(windows)]
pub fn pid_is_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }

    // kernel32 exports; see the Win32 API docs for signatures.
    extern "system" {
        fn OpenProcess(dwDesiredAccess: u32, bInheritHandle: i32, dwProcessId: u32) -> isize;
        fn GetExitCodeProcess(hProcess: isize, lpExitCode: *mut u32) -> i32;
        fn CloseHandle(hObject: isize) -> i32;
    }

    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const STILL_ACTIVE: u32 = 259;

    // SAFETY: straightforward FFI — open a query-only handle, read the
    // exit code, close the handle. All pointers are valid for the call.
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle == 0 {
            // Couldn't open: the process is almost certainly gone.
            return false;
        }
        let mut code: u32 = 0;
        let ok = GetExitCodeProcess(handle, &mut code);
        CloseHandle(handle);
        ok != 0 && code == STILL_ACTIVE
    }
}
