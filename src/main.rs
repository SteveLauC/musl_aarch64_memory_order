#![allow(rust_2024_compatibility)]

use nix::errno::Errno;
use nix::libc;
use nix::libc::c_int;
use nix::sys::aio::*;
use nix::sys::signal::*;
use std::os::fd::AsRawFd;
use std::os::unix::io::AsFd;
use tempfile::tempfile;

fn main() {
    static mut PIPE_TX_FD: c_int = -1;
    let (rx, tx) = nix::unistd::pipe().unwrap();
    unsafe {
        PIPE_TX_FD = tx.as_raw_fd();
    }

    extern "C" fn sigfunc(_: c_int) {
        let dbg_msg = "DBG: signaled\n";
        let dbg_msg_len = dbg_msg.len();

        // Writing to stdout is okay for debugging, but can be risky.
        let _ = unsafe { libc::write(1, dbg_msg.as_ptr().cast(), dbg_msg_len as _) };

        let fd = unsafe { PIPE_TX_FD };
        assert_ne!(fd, -1);
        let res = unsafe { libc::write(fd, dbg_msg.as_ptr().cast(), dbg_msg_len as _) };
        if res == -1 {
            // Can't do much here, aborting is an option but let's avoid it.
        }
    }
    let sa = SigAction::new(
        SigHandler::Handler(sigfunc),
        SaFlags::SA_RESETHAND,
        SigSet::empty(),
    );
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

    let mut buf = [0_u8; 128];

    loop {
        match nix::unistd::read(&rx, &mut buf) {
            Ok(n_read) => {
                println!(
                    "DBG: read {} bytes from pipe, content: [{}]",
                    n_read,
                    std::str::from_utf8(&buf[..n_read]).unwrap_or("invalid utf8")
                );

                break;
            }
            Err(Errno::EINTR) => {
                continue;
            }
            Err(e) => {
                println!("DBG: read() failed with {}", e);
            }
        }
    }

    // At this point, since `lio_listio` returned success and delivered its
    // notification, we know that all operations are complete.
    assert_eq!(aiow.as_mut().aio_return().unwrap(), WBUF.len());
}
