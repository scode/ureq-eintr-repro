# ureq-eintr-repro

A minimal reproducer for [ureq](https://github.com/algesten/ureq) 3.4.2 failing a request with
`Interrupted system call (os error 4)` when a signal lands while it waits for a response.

## The bug

When a timeout is configured, ureq sets `SO_RCVTIMEO` on its socket. On Linux a socket read with a receive timeout is
not restarted after a signal interrupts it; it fails with `EINTR` (see signal(7)). ureq's `TcpTransport::await_input`
passes that `ErrorKind::Interrupted` up as a request failure instead of retrying the read.

No signal handler is needed to trigger it. glibc's `posix_spawn`, which `std::process::Command` uses, blocks all
signals in the spawning thread, so a `SIGCHLD` arriving then is queued rather than discarded and can wake another
thread's blocked read. In practice: any process that spawns subprocesses while another thread makes ureq requests, such
as a test binary where some tests run commands and others talk HTTP.

## Reproducing

On Linux:

```sh
cargo test
```

The test fails within a couple of seconds with `request failed: io: Interrupted system call (os error 4)`. It runs a
local HTTP server that answers after a short delay, a few threads spawning `true` in a loop, and up to 2000 ureq
requests. Setting the timeout to `None` in `tests/eintr.rs` makes it pass, since the read is then restarted
transparently.
