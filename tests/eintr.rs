//! ureq 3 surfaces EINTR from a socket read as a request failure.
//!
//! When a timeout is configured, ureq sets SO_RCVTIMEO on the socket. Linux
//! does not restart a socket read with a receive timeout after a signal
//! interrupts it; the read fails with EINTR instead (see signal(7), "Interruption
//! of system calls and library functions by signal handlers"). This happens
//! even when the process installs no signal handler at all: glibc's
//! posix_spawn (what std::process::Command uses) blocks every signal in the
//! spawning thread, so a SIGCHLD from an exiting child arrives while it is
//! blocked there, is queued instead of discarded, and wakes another thread's
//! blocked read. ureq's TcpTransport::await_input passes that
//! ErrorKind::Interrupted straight up instead of retrying the read.
//!
//! The test runs a local HTTP server that answers after a short delay, so the
//! client sits in read(), while other threads spawn `true` in a loop. Every
//! request should succeed; with ureq 3.4.2 some fail with "Interrupted system
//! call (os error 4)". Linux only.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

#[test]
fn requests_survive_sigchld_from_concurrent_spawns() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            while reader.read_line(&mut line).unwrap() > 2 {
                line.clear();
            }
            thread::sleep(Duration::from_millis(2));
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .unwrap();
        }
    });

    let stop = Arc::new(AtomicBool::new(false));
    let spawners: Vec<_> = (0..4)
        .map(|_| {
            let stop = stop.clone();
            thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    Command::new("true").status().unwrap();
                }
            })
        })
        .collect();

    // Any timeout will do; its only role is to make ureq set SO_RCVTIMEO.
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(60)))
        .build()
        .into();
    let failure = (0..2000).find_map(|_| agent.get(&url).call().err());

    stop.store(true, Ordering::Relaxed);
    for s in spawners {
        s.join().unwrap();
    }
    if let Some(e) = failure {
        panic!("request failed: {e}");
    }
}
