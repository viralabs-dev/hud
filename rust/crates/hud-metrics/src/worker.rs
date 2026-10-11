//! One worker, one in-flight sample; polling never waits for I/O.
use crate::{RawSample, Sample, Sampler};
use std::{
    io,
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::Instant,
};
#[derive(Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);
impl Cancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub enum Shutdown {
    #[default]
    DetachNativeIo,
    BoundedProcessCleanup,
}
pub trait MetricsSource: Send + 'static {
    fn shutdown_kind(&self) -> Shutdown {
        Shutdown::DetachNativeIo
    }
    /// Sources should cooperate with cancellation. Native calls cannot be forcibly interrupted.
    fn read(&mut self, cancel: &Cancellation) -> io::Result<RawSample>;
}
impl MetricsSource for crate::LinuxBackend {
    fn read(&mut self, cancel: &Cancellation) -> io::Result<RawSample> {
        if cancel.is_cancelled() {
            return Err(io::ErrorKind::Interrupted.into());
        }
        Ok(crate::LinuxBackend::read(self))
    }
}
struct Request {
    token: Cancellation,
}
struct Reply {
    token: Cancellation,
    at: f64,
    raw: io::Result<RawSample>,
}
pub struct MetricsWorker {
    request: Option<mpsc::SyncSender<Request>>,
    worker: Option<std::thread::JoinHandle<()>>,
    shutdown: Shutdown,
    result: mpsc::Receiver<Reply>,
    pending: Option<Cancellation>,
    pub sampler: Sampler,
    pub latest: Sample,
    pub last_error: Option<String>,
}
impl MetricsWorker {
    pub fn new(mut source: impl MetricsSource, history: usize) -> io::Result<Self> {
        let (request, receive) = mpsc::sync_channel::<Request>(1);
        let (send, result) = mpsc::channel();
        let start = Instant::now();
        let shutdown = source.shutdown_kind();
        let worker = std::thread::Builder::new()
            .name("hud-metrics".into())
            .spawn(move || {
                while let Ok(req) = receive.recv() {
                    let raw = catch_unwind(AssertUnwindSafe(|| source.read(&req.token)))
                        .unwrap_or_else(|_| Err(io::Error::other("metrics source panicked")));
                    if send
                        .send(Reply {
                            token: req.token,
                            at: start.elapsed().as_secs_f64(),
                            raw,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })?;
        Ok(Self {
            request: Some(request),
            worker: Some(worker),
            shutdown,
            result,
            pending: None,
            sampler: Sampler::new(history),
            latest: Sample::default(),
            last_error: None,
        })
    }
    /// False means a sample is already running or unpolled; requests coalesce.
    pub fn request(&mut self) -> bool {
        if self.pending.is_some() {
            return false;
        }
        let token = Cancellation::default();
        let Some(tx) = &self.request else {
            return false;
        };
        if tx
            .try_send(Request {
                token: token.clone(),
            })
            .is_err()
        {
            return false;
        }
        self.pending = Some(token);
        true
    }
    pub fn is_busy(&self) -> bool {
        self.pending.is_some()
    }
    pub fn cancel(&self) {
        if let Some(t) = &self.pending {
            t.cancel();
        }
    }
    /// True when a completed result was consumed. Cancelled results are discarded.
    pub fn poll(&mut self) -> bool {
        match self.result.try_recv() {
            Ok(reply) => {
                self.pending = None;
                if reply.token.is_cancelled() {
                    return true;
                }
                let raw = match reply.raw {
                    Ok(raw) => {
                        self.last_error = None;
                        raw
                    }
                    Err(e) => {
                        self.last_error = Some(e.to_string());
                        RawSample::default()
                    }
                };
                self.latest = self.sampler.sample(raw, reply.at);
                true
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                if self.pending.take().is_some() {
                    self.last_error = Some("metrics worker disconnected".into());
                    return true;
                }
                false
            }
            Err(mpsc::TryRecvError::Empty) => false,
        }
    }
}
impl Drop for MetricsWorker {
    fn drop(&mut self) {
        self.cancel();
        self.request.take();
        if let Some(handle) = self.worker.take() {
            if matches!(self.shutdown, Shutdown::BoundedProcessCleanup) {
                let deadline = Instant::now() + std::time::Duration::from_secs(2);
                while !handle.is_finished() && Instant::now() < deadline {
                    std::thread::sleep(std::time::Duration::from_millis(2));
                }
            }
            if handle.is_finished() {
                let _ = handle.join();
            }
            // Native I/O cannot be killed safely. If still blocked after bounded
            // process cleanup budget, detach; no claim of forced OS-call shutdown.
        }
    }
}
/// Selects only the host backend; no probing commands for a foreign OS.
pub fn platform_source(
    paths: Vec<(std::path::PathBuf, String)>,
) -> io::Result<Box<dyn MetricsSource>> {
    #[cfg(target_os = "linux")]
    {
        Ok(Box::new(
            crate::LinuxBackend::default().with_disk_paths(paths),
        ))
    }
    #[cfg(target_os = "macos")]
    {
        Ok(Box::new(crate::platforms::macos::Backend::new(paths)))
    }
    #[cfg(target_os = "windows")]
    {
        Ok(Box::new(crate::platforms::windows::Backend::new(paths)))
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let _ = paths;
        Err(io::ErrorKind::Unsupported.into())
    }
}
impl MetricsSource for Box<dyn MetricsSource> {
    fn shutdown_kind(&self) -> Shutdown {
        (**self).shutdown_kind()
    }
    fn read(&mut self, c: &Cancellation) -> io::Result<RawSample> {
        (**self).read(c)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::atomic::AtomicUsize, time::Duration};
    struct Fake {
        count: Arc<AtomicUsize>,
        delay: Duration,
        panic_first: bool,
    }
    impl MetricsSource for Fake {
        fn read(&mut self, c: &Cancellation) -> io::Result<RawSample> {
            let n = self.count.fetch_add(1, Ordering::SeqCst);
            if self.panic_first && n == 0 {
                panic!("fixture panic");
            }
            let until = Instant::now() + self.delay;
            while Instant::now() < until {
                if c.is_cancelled() {
                    return Err(io::ErrorKind::Interrupted.into());
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            Ok(RawSample {
                cpu: Some((n as u64, 10 * n as u64)),
                network: Some((n as u64 * 100, n as u64 * 50)),
                sample: Sample {
                    cores: Some(4),
                    ..Sample::default()
                },
            })
        }
    }
    fn complete(w: &mut MetricsWorker) {
        let end = Instant::now() + Duration::from_secs(2);
        while !w.poll() {
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    #[test]
    fn slow_coalesces_nonblocking_and_history() {
        struct Gate {
            release: mpsc::Receiver<()>,
            count: usize,
        }
        impl MetricsSource for Gate {
            fn read(&mut self, _: &Cancellation) -> io::Result<RawSample> {
                self.release.recv().unwrap();
                self.count += 1;
                Ok(RawSample {
                    cpu: Some((self.count as u64, self.count as u64 * 10)),
                    network: Some((self.count as u64 * 100, self.count as u64 * 50)),
                    ..RawSample::default()
                })
            }
        }
        let (tx, rx) = mpsc::channel();
        let mut w = MetricsWorker::new(
            Gate {
                release: rx,
                count: 0,
            },
            2,
        )
        .unwrap();
        assert!(w.request());
        for _ in 0..100 {
            assert!(!w.request());
            assert!(!w.poll());
        }
        tx.send(()).unwrap();
        complete(&mut w);
        assert_eq!(w.latest.cpu_percent, None);
        assert!(w.request());
        tx.send(()).unwrap();
        complete(&mut w);
        assert_eq!(w.latest.cpu_percent, Some(90.));
        assert!(w.latest.network_bytes_per_second.is_some());
        assert_eq!(w.sampler.cpu_history.len(), 2);
    }
    #[test]
    fn cancel_discards_and_next_read_recovers() {
        let mut w = MetricsWorker::new(
            Fake {
                count: Arc::new(AtomicUsize::new(0)),
                delay: Duration::from_millis(15),
                panic_first: false,
            },
            2,
        )
        .unwrap();
        assert!(w.request());
        w.cancel();
        complete(&mut w);
        assert!(w.sampler.cpu_history.is_empty());
        assert_eq!(w.latest.cores, None);
        assert!(w.request());
        complete(&mut w);
        assert_eq!(w.latest.cores, Some(4));
    }
    #[test]
    fn panic_yields_unavailable_then_worker_recovers() {
        let mut w = MetricsWorker::new(
            Fake {
                count: Arc::new(AtomicUsize::new(0)),
                delay: Duration::ZERO,
                panic_first: true,
            },
            2,
        )
        .unwrap();
        assert!(w.request());
        complete(&mut w);
        assert!(w.last_error.as_deref().unwrap().contains("panicked"));
        assert_eq!(w.latest.cpu_percent, None);
        assert!(w.request());
        complete(&mut w);
        assert_eq!(w.latest.cores, Some(4));
        assert!(w.last_error.is_none());
    }
    #[test]
    fn drop_does_not_join_slow_source() {
        struct Gate(mpsc::Receiver<()>);
        impl MetricsSource for Gate {
            fn read(&mut self, _: &Cancellation) -> io::Result<RawSample> {
                let _ = self.0.recv();
                Ok(RawSample::default())
            }
        }
        let (tx, rx) = mpsc::channel();
        let mut w = MetricsWorker::new(Gate(rx), 2).unwrap();
        assert!(w.request());
        drop(w); // A blocking join would deadlock before releasing the gate.
        let _ = tx.send(());
    }
    #[test]
    fn source_error_resets_baselines_before_recovery() {
        struct Errors(usize);
        impl MetricsSource for Errors {
            fn read(&mut self, _: &Cancellation) -> io::Result<RawSample> {
                self.0 += 1;
                if self.0 == 2 {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "malformed fixture",
                    ));
                }
                Ok(RawSample {
                    cpu: Some((self.0 as u64, self.0 as u64 * 10)),
                    ..RawSample::default()
                })
            }
        }
        let mut w = MetricsWorker::new(Errors(0), 3).unwrap();
        assert!(w.request());
        complete(&mut w);
        assert!(w.request());
        complete(&mut w);
        assert!(w.last_error.is_some());
        assert_eq!(w.latest.cpu_percent, None);
        assert!(w.request());
        complete(&mut w);
        assert!(w.last_error.is_none());
        assert_eq!(w.latest.cpu_percent, None);
        assert_eq!(w.sampler.cpu_history.len(), 3);
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn actual_linux_platform_selected_without_foreign_probes() {
        let source = platform_source(vec![(std::path::PathBuf::from("/"), "root".into())]).unwrap();
        let mut w = MetricsWorker::new(source, 2).unwrap();
        assert!(w.request());
        complete(&mut w);
        assert!(w.latest.memory.as_ref().is_some_and(|m| m.total > 0));
        assert!(w
            .latest
            .disks
            .iter()
            .any(|d| d.label == "root" && d.total > 0));
        assert!(w.last_error.is_none());
    }
    #[cfg(unix)]
    #[test]
    fn subprocess_source_cleanup_finishes_before_drop_returns() {
        struct ChildSource {
            started: Arc<AtomicBool>,
            cleaned: Arc<AtomicBool>,
        }
        impl MetricsSource for ChildSource {
            fn shutdown_kind(&self) -> Shutdown {
                Shutdown::BoundedProcessCleanup
            }
            fn read(&mut self, c: &Cancellation) -> io::Result<RawSample> {
                let mut child = std::process::Command::new("/bin/sleep").arg("5").spawn()?;
                self.started.store(true, Ordering::Release);
                while !c.is_cancelled() {
                    std::thread::sleep(Duration::from_millis(2));
                }
                child.kill()?;
                let _ = child.wait()?;
                self.cleaned.store(true, Ordering::Release);
                Err(io::ErrorKind::Interrupted.into())
            }
        }
        let started = Arc::new(AtomicBool::new(false));
        let cleaned = Arc::new(AtomicBool::new(false));
        let mut w = MetricsWorker::new(
            ChildSource {
                started: started.clone(),
                cleaned: cleaned.clone(),
            },
            2,
        )
        .unwrap();
        assert!(w.request());
        let deadline = Instant::now() + Duration::from_secs(2);
        while !started.load(Ordering::Acquire) {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        drop(w);
        assert!(cleaned.load(Ordering::Acquire));
    }
}
