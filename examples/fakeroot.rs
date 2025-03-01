extern crate unshare;

use std::process::exit;


fn main() {
    let mut cmd = unshare::Command::new("/usr/bin/ls");
    cmd.arg("-l");
    cmd.arg("/");

    cmd.fakeroot_enable("/dev/shm/sandbox_root");
    cmd.fakeroot_mount("/bin", "/bin", true);
    cmd.fakeroot_mount("/etc", "/etc", true);
    cmd.fakeroot_mount("/lib", "/lib", true);
    cmd.fakeroot_mount("/lib64", "/lib64", true);
    cmd.fakeroot_mount("/usr", "/usr", true);
    cmd.current_dir("/");

    match cmd.status().unwrap() {
        // propagate signal
        unshare::ExitStatus::Exited(x) => exit(x as i32),
        unshare::ExitStatus::Signaled(x, _) => exit((128+x as i32) as i32),
    }
}
