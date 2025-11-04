use nix::libc::c_int;
use nix::sys::aio::*;
use nix::sys::signal::*;
use std::os::unix::io::AsFd;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time;
use tempfile::tempfile;

pub static SIGNALED: AtomicBool = AtomicBool::new(false);

fn main() {
    extern "C" fn sigfunc(_: c_int) {
        SIGNALED.store(true, Ordering::Release);
    }
    let sa = SigAction::new(
        SigHandler::Handler(sigfunc),
        SaFlags::SA_RESETHAND,
        SigSet::empty(),
    );
    SIGNALED.store(false, Ordering::Release);
    unsafe { sigaction(Signal::SIGUSR2, &sa) }.unwrap();

    const WBUF: &[u8] = b"abcdef123456";
    let f = tempfile().unwrap();
    let mut aiow = Box::pin(AioWrite::new(
        f.as_fd(),
        2, // offset
        WBUF,
        0, // priority
        SigevNotify::SigevNone,
    ));
    let sev = SigevNotify::SigevSignal {
        signal: Signal::SIGUSR2,
        si_value: 0,
    };

    #[allow(deprecated)]
    lio_listio(LioMode::LIO_NOWAIT, &mut [aiow.as_mut()], sev).unwrap();

    while !SIGNALED.load(Ordering::Acquire) {
        thread::sleep(time::Duration::from_millis(10));
    }
    // At this point, since `lio_listio` returned success and delivered its
    // notification, we know that all operations are complete.
    assert_eq!(aiow.as_mut().aio_return().unwrap(), WBUF.len());
}
