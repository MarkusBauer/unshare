use crate::ffi_util::ToCString;
use crate::{Command, Namespace};
use libc::{MNT_DETACH, MS_BIND, MS_PRIVATE, MS_RDONLY, MS_REC, MS_REMOUNT};
use std::ffi::{c_char, c_void, CString};
use std::path::Path;

pub struct FakeRootMount {
    mountpoint: CString,
    mountpoint_outer: CString,
    src: CString,
    readonly: bool,
}

impl Command {
    /// Enable "fakeroot" - the command will be rooted in a custom root directory.
    ///
    /// By default, the root directory is empty, share necessary directories with fakeroot_mount().
    /// This will automatically unshare the mount namespace.
    /// It might be necessary to also unshare the user namespace.
    ///
    /// The "base" directory must be an empty directory, preferably on a tmpfs.
    /// The directory will be created if missing.
    /// "/dev/shm/unshare_root" should work fine, or "/run/user/<uid>/unshare_root".
    ///
    /// Do NOT combine with manual pivot_root/chroot, fakeroot will take care of it.
    pub fn fakeroot_enable(&mut self, base: &str) {
        self.unshare(&[Namespace::Mount]);
        self.config.fake_root_base = Some(base.to_cstring());
        self.config.fake_root_proc = Some(format!("{}/proc", base).to_cstring());
    }

    /// Add an existing directory to the fakeroot.
    ///
    /// fakeroot_enable() must be called first, otherwise this function will panic.
    ///
    /// Example usage:
    ///   cmd.fakeroot_mount("/bin", "/bin", true);
    ///   cmd.fakeroot_mount("/etc", "/etc", true);
    ///   cmd.fakeroot_mount("/lib", "/lib", true);
    ///   cmd.fakeroot_mount("/lib64", "/lib64", true);
    ///   cmd.fakeroot_mount("/usr", "/usr", true);
    pub fn fakeroot_mount<P: AsRef<Path>>(&mut self, src: P, dst: &str, readonly: bool) {
        let base = self
            .config
            .fake_root_base
            .as_ref()
            .expect("call fakeroot_enable() first!")
            .to_str()
            .unwrap();
        self.config.fake_root_mounts.push(FakeRootMount {
            mountpoint: dst.to_cstring(),
            mountpoint_outer: format!("{}/{}", base, dst).to_cstring(),
            src: src.as_ref().to_cstring(),
            readonly,
        });
    }
}

/// This syscall sequence is more or less taken from nsjail (https://github.com/google/nsjail).
pub(crate) unsafe fn build_fakeroot(
    base: &CString,
    proc: Option<&CString>,
    mountpoints: &[FakeRootMount],
) -> bool {
    // define some libc constants
    let null_char = 0 as *const c_char;
    let null_void = 0 as *const c_void;
    let slash = b"/\0".as_ptr() as *const c_char;
    let dot = b".\0".as_ptr() as *const c_char;
    let tmpfs = b"tmpfs\0".as_ptr() as *const c_char;
    let procfs = b"proc\0".as_ptr() as *const c_char;

    // keep all mount changes private
    libc::mkdir(base.as_ptr(), 0o777);
    if libc::mount(slash, slash, null_char, MS_PRIVATE | MS_REC, null_void) < 0 {
        return false;
    }

    // create fakeroot filesystem
    if libc::mount(null_char, base.as_ptr(), tmpfs, 0, null_void) < 0 {
        return false;
    }

    // mount directories - still read-write (because MS_BIND + MS_RDONLY are not supported)
    for mount in mountpoints {
        libc::mkdir(mount.mountpoint_outer.as_ptr(), 0o777);
        if libc::mount(
            mount.src.as_ptr(),
            mount.mountpoint_outer.as_ptr(),
            null_char,
            MS_PRIVATE | MS_REC | MS_BIND,
            null_void,
        ) < 0
        {
            return false;
        }
    }

    // mount new "/proc" (if available, for example: not in docker). No error if failing.
    proc.map(|proc| {
        libc::mkdir(proc.as_ptr(), 0o777);
        libc::mount(null_char, proc.as_ptr(), procfs, 0, null_void);
    });

    // chroot jail (try pivot_root first, use classic chroot if not available)
    if libc::syscall(libc::SYS_pivot_root, base.as_ptr(), base.as_ptr()) >= 0 {
        libc::umount2(slash, MNT_DETACH);
    } else {
        libc::chdir(base.as_ptr());
        libc::mount(dot, slash, null_char, 0, null_void);
        if libc::chroot(dot) < 0 {
            return false;
        }
    }

    // make directories actually read-only
    libc::mount(
        slash,
        slash,
        null_char,
        MS_REMOUNT | MS_BIND | MS_RDONLY,
        null_void,
    );
    for mount in mountpoints {
        if mount.readonly {
            if libc::mount(
                mount.mountpoint.as_ptr(),
                mount.mountpoint.as_ptr(),
                null_char,
                MS_REMOUNT | MS_BIND | MS_RDONLY,
                null_void,
            ) < 0
            {
                return false;
            }
        }
    }

    true
}
