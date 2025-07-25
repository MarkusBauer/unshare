use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::signal::unix::{signal, SignalKind};
use crate::{Child, ExitStatus};

pub struct ChildWaiter<'a> {
    child: &'a mut Child,
    signal: tokio::signal::unix::Signal,
}

impl Future for ChildWaiter<'_> {
    type Output = io::Result<ExitStatus>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // mostly borrowed from tokio's command/process handling
        loop {
            let registered_interest = self.signal.poll_recv(cx).is_pending();

            if let Some(status) = self.child.try_wait()? {
                return Poll::Ready(Ok(status));
            }

            if registered_interest {
                return Poll::Pending;
            } else {
                continue;
            }
        }
    }
}

/// Get a future that completes when the process terminates
pub fn wait_async(child: &mut Child) -> ChildWaiter {
    ChildWaiter {
        child,
        signal: signal(SignalKind::child()).expect("failed to register signal handler"),
    }
}
