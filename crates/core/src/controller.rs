//! Pengatur pengambilan lirik: cache per `track_key`, fetch di thread
//! terpisah, retry otomatis saat error jaringan, dan membuang hasil basi.
//!
//! [`LyricsController`] dipanggil dari thread UI dan tidak pernah blocking:
//! request dikirim ke thread worker lewat channel, hasilnya diambil lagi
//! lewat [`LyricsController::poll`].

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::lrclib::LrclibClient;
use crate::model::{FetchOutcome, LyricsStatus, PlayerState, TrackQuery};

/// Jeda retry otomatis setelah error jaringan.
pub const RETRY_AFTER: Duration = Duration::from_secs(30);

/// Sumber lirik (LRCLIB di produksi, palsu di test).
pub trait LyricsProvider: Send + 'static {
    fn fetch(&self, query: &TrackQuery) -> FetchOutcome;
}

impl LyricsProvider for LrclibClient {
    fn fetch(&self, query: &TrackQuery) -> FetchOutcome {
        LrclibClient::fetch(self, query)
    }
}

struct Job {
    generation: u64,
    query: TrackQuery,
}

struct JobResult {
    generation: u64,
    track_key: String,
    outcome: FetchOutcome,
    latency: Duration,
}

/// Status lirik yang siap ditampilkan UI.
#[derive(Debug, Clone)]
pub struct LyricsView {
    pub track_key: Option<String>,
    pub status: LyricsStatus,
    /// Latency request terakhir yang selesai untuk lagu ini.
    pub latency: Option<Duration>,
    /// Kapan retry berikutnya dijadwalkan (hanya saat NetworkError).
    pub retry_at: Option<Instant>,
    /// Berapa kali sudah mencoba untuk lagu ini.
    pub attempts: u32,
}

impl Default for LyricsView {
    fn default() -> Self {
        Self {
            track_key: None,
            status: LyricsStatus::Idle,
            latency: None,
            retry_at: None,
            attempts: 0,
        }
    }
}

pub struct LyricsController {
    jobs: Sender<Job>,
    results: Receiver<JobResult>,
    /// Hanya Ready dan NotFound yang di-cache. NetworkError tidak.
    cache: HashMap<String, LyricsStatus>,
    view: LyricsView,
    current_query: Option<TrackQuery>,
    generation: u64,
    in_flight: bool,
}

impl LyricsController {
    /// Buat controller dengan thread worker. `on_result` dipanggil dari thread
    /// worker setiap kali ada hasil (mis. untuk `ctx.request_repaint()`).
    pub fn new<P: LyricsProvider>(provider: P, on_result: impl Fn() + Send + 'static) -> Self {
        let (job_tx, job_rx) = mpsc::channel::<Job>();
        let (res_tx, res_rx) = mpsc::channel::<JobResult>();
        thread::Builder::new()
            .name("lyrics-fetch".into())
            .spawn(move || worker(provider, job_rx, res_tx, on_result))
            .expect("spawn lyrics worker");
        Self {
            jobs: job_tx,
            results: res_rx,
            cache: HashMap::new(),
            view: LyricsView::default(),
            current_query: None,
            generation: 0,
            in_flight: false,
        }
    }

    pub fn view(&self) -> &LyricsView {
        &self.view
    }

    /// Panggil tiap frame dengan state player terbaru. Murah: hanya
    /// membandingkan string dan lookup HashMap.
    pub fn update(&mut self, player: Option<&PlayerState>, now: Instant) {
        self.poll();
        let Some(player) = player else {
            if self.view.track_key.is_some() {
                self.generation += 1;
                self.current_query = None;
                self.in_flight = false;
                self.view = LyricsView::default();
            }
            return;
        };

        if self.view.track_key.as_deref() != Some(player.track_key.as_str()) {
            self.generation += 1;
            self.in_flight = false;
            self.current_query = Some(player.query());
            self.view = LyricsView {
                track_key: Some(player.track_key.clone()),
                ..LyricsView::default()
            };
            if let Some(cached) = self.cache.get(&player.track_key) {
                log::debug!(
                    "lirik dari cache: {} ({})",
                    player.track_key,
                    cached.label()
                );
                self.view.status = cached.clone();
            } else {
                self.view.status = LyricsStatus::Loading;
                self.dispatch();
            }
            return;
        }

        if let (LyricsStatus::NetworkError(_), Some(retry_at)) =
            (&self.view.status, self.view.retry_at)
        {
            if now >= retry_at && !self.in_flight {
                log::info!("retry fetch lirik: {}", player.track_key);
                self.view.retry_at = None;
                self.dispatch();
            }
        }
    }

    fn dispatch(&mut self) {
        let Some(query) = self.current_query.clone() else {
            return;
        };
        self.in_flight = true;
        self.view.attempts += 1;
        let job = Job {
            generation: self.generation,
            query,
        };
        if self.jobs.send(job).is_err() {
            log::error!("thread fetch lirik mati");
            self.in_flight = false;
            self.view.status = LyricsStatus::NetworkError("worker lirik berhenti".into());
        }
    }

    /// Ambil hasil dari worker. Dipanggil otomatis oleh [`Self::update`].
    pub fn poll(&mut self) {
        while let Ok(res) = self.results.try_recv() {
            self.apply(res, Instant::now());
        }
    }

    /// Tunggu satu hasil (khusus test).
    #[doc(hidden)]
    pub fn poll_blocking(&mut self, timeout: Duration) -> bool {
        match self.results.recv_timeout(timeout) {
            Ok(res) => {
                self.apply(res, Instant::now());
                true
            }
            Err(_) => false,
        }
    }

    fn apply(&mut self, res: JobResult, now: Instant) {
        if res.generation != self.generation {
            log::debug!("buang hasil lirik basi untuk {}", res.track_key);
            return;
        }
        self.in_flight = false;
        self.view.latency = Some(res.latency);
        match res.outcome {
            FetchOutcome::Found(lines) => {
                let status = LyricsStatus::Ready(Arc::new(lines));
                self.cache.insert(res.track_key, status.clone());
                self.view.status = status;
                self.view.retry_at = None;
            }
            FetchOutcome::NotFound => {
                self.cache.insert(res.track_key, LyricsStatus::NotFound);
                self.view.status = LyricsStatus::NotFound;
                self.view.retry_at = None;
            }
            FetchOutcome::NetworkError(msg) => {
                self.view.status = LyricsStatus::NetworkError(msg);
                self.view.retry_at = Some(now + RETRY_AFTER);
            }
        }
    }

    #[cfg(test)]
    fn force_retry_now(&mut self) {
        if self.view.retry_at.is_some() {
            self.view.retry_at = Some(Instant::now());
        }
    }
}

fn worker<P: LyricsProvider>(
    provider: P,
    jobs: Receiver<Job>,
    results: Sender<JobResult>,
    on_result: impl Fn(),
) {
    while let Ok(mut job) = jobs.recv() {
        // Kalau ada beberapa request menumpuk, cukup kerjakan yang terbaru.
        while let Ok(newer) = jobs.try_recv() {
            job = newer;
        }
        let q = &job.query;
        log::info!(
            "fetch lirik: \"{}\" — \"{}\" [{}] {:.0}s",
            q.title,
            q.artist,
            q.album,
            q.duration_secs
        );
        let started = Instant::now();
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| provider.fetch(q)))
            .unwrap_or_else(|_| FetchOutcome::NetworkError("panic saat fetch lirik".into()));
        let latency = started.elapsed();
        match &outcome {
            FetchOutcome::Found(lines) => {
                log::info!(
                    "lirik ketemu: {} baris ({} ms)",
                    lines.len(),
                    latency.as_millis()
                )
            }
            FetchOutcome::NotFound => {
                log::info!("lirik sinkron tidak ditemukan ({} ms)", latency.as_millis())
            }
            FetchOutcome::NetworkError(e) => {
                log::warn!("error jaringan LRCLIB ({} ms): {e}", latency.as_millis())
            }
        }
        let res = JobResult {
            generation: job.generation,
            track_key: job.query.track_key,
            outcome,
            latency,
        };
        if results.send(res).is_err() {
            break;
        }
        on_result();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::LyricLine;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    /// Provider palsu: hasil diambil berurutan dari daftar, dan bisa ditahan
    /// dengan gate supaya kita bisa mensimulasikan request yang masih jalan.
    struct Fake {
        outcomes: Arc<Mutex<Vec<FetchOutcome>>>,
        calls: Arc<AtomicUsize>,
        gate: Option<Arc<Mutex<Receiver<()>>>>,
        /// Kalau diisi, hasil ditentukan dari track_key, bukan urutan panggilan.
        by_key: Option<fn(&str) -> FetchOutcome>,
    }

    impl LyricsProvider for Fake {
        fn fetch(&self, q: &TrackQuery) -> FetchOutcome {
            if let Some(g) = &self.gate {
                let _ = g.lock().unwrap().recv();
            }
            self.calls.fetch_add(1, Ordering::SeqCst);
            if let Some(f) = self.by_key {
                return f(&q.track_key);
            }
            let mut o = self.outcomes.lock().unwrap();
            if o.is_empty() {
                FetchOutcome::NotFound
            } else {
                o.remove(0)
            }
        }
    }

    fn fake(outcomes: Vec<FetchOutcome>) -> (Fake, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        (
            Fake {
                outcomes: Arc::new(Mutex::new(outcomes)),
                calls: calls.clone(),
                gate: None,
                by_key: None,
            },
            calls,
        )
    }

    fn player(key: &str) -> PlayerState {
        PlayerState {
            source_app: "test".into(),
            track_key: key.into(),
            title: key.into(),
            artist: "A".into(),
            album: "B".into(),
            duration_secs: 100.0,
            position_secs: 0.0,
            position_sampled_at: Instant::now(),
            is_playing: true,
        }
    }

    fn found() -> FetchOutcome {
        FetchOutcome::Found(vec![LyricLine {
            time_secs: 1.0,
            text: "x".into(),
        }])
    }

    const WAIT: Duration = Duration::from_secs(5);

    #[test]
    fn fetches_once_per_track_and_caches() {
        let (p, calls) = fake(vec![found(), FetchOutcome::NotFound]);
        let mut c = LyricsController::new(p, || {});
        let a = player("a");
        c.update(Some(&a), Instant::now());
        assert_eq!(c.view().status, LyricsStatus::Loading);
        assert!(c.poll_blocking(WAIT));
        assert!(matches!(c.view().status, LyricsStatus::Ready(_)));
        // Update berulang dengan lagu sama tidak fetch lagi.
        c.update(Some(&a), Instant::now());
        c.update(Some(&a), Instant::now());

        let b = player("b");
        c.update(Some(&b), Instant::now());
        assert!(c.poll_blocking(WAIT));
        assert_eq!(c.view().status, LyricsStatus::NotFound);

        // Kembali ke lagu a & b: dari cache, tanpa fetch.
        c.update(Some(&a), Instant::now());
        assert!(matches!(c.view().status, LyricsStatus::Ready(_)));
        c.update(Some(&b), Instant::now());
        assert_eq!(c.view().status, LyricsStatus::NotFound);
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn network_error_is_not_cached_and_retries() {
        let (p, calls) = fake(vec![FetchOutcome::NetworkError("timeout".into()), found()]);
        let mut c = LyricsController::new(p, || {});
        let a = player("a");
        c.update(Some(&a), Instant::now());
        assert!(c.poll_blocking(WAIT));
        assert!(matches!(c.view().status, LyricsStatus::NetworkError(_)));
        let retry_at = c.view().retry_at.expect("retry dijadwalkan");
        assert!(retry_at >= Instant::now() + RETRY_AFTER - Duration::from_secs(1));

        // Belum waktunya retry.
        c.update(Some(&a), Instant::now());
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        c.force_retry_now();
        c.update(Some(&a), Instant::now());
        assert!(c.poll_blocking(WAIT));
        assert!(matches!(c.view().status, LyricsStatus::Ready(_)));
        assert_eq!(c.view().attempts, 2);
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn network_error_for_track_is_refetched_when_revisited() {
        let (p, calls) = fake(vec![
            FetchOutcome::NetworkError("dns".into()),
            found(),
            found(),
        ]);
        let mut c = LyricsController::new(p, || {});
        c.update(Some(&player("a")), Instant::now());
        assert!(c.poll_blocking(WAIT));
        c.update(Some(&player("b")), Instant::now());
        assert!(c.poll_blocking(WAIT));
        c.update(Some(&player("a")), Instant::now());
        assert_eq!(c.view().status, LyricsStatus::Loading);
        assert!(c.poll_blocking(WAIT));
        assert!(matches!(c.view().status, LyricsStatus::Ready(_)));
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn stale_result_is_discarded() {
        let (gate_tx, gate_rx) = mpsc::channel();
        let (mut p, _calls) = fake(vec![]);
        p.gate = Some(Arc::new(Mutex::new(gate_rx)));
        p.by_key = Some(|k| {
            if k == "a" {
                found()
            } else {
                FetchOutcome::NotFound
            }
        });
        let mut c = LyricsController::new(p, || {});

        c.update(Some(&player("a")), Instant::now());
        // Lagu ganti sebelum request "a" selesai.
        c.update(Some(&player("b")), Instant::now());
        gate_tx.send(()).unwrap();
        gate_tx.send(()).unwrap();

        // Hasil untuk "a" (Found) harus dibuang; tunggu hasil "b".
        let deadline = Instant::now() + WAIT;
        while c.view().status == LyricsStatus::Loading && Instant::now() < deadline {
            c.poll_blocking(Duration::from_millis(100));
        }
        assert_eq!(c.view().track_key.as_deref(), Some("b"));
        assert_eq!(c.view().status, LyricsStatus::NotFound);
    }

    #[test]
    fn no_player_resets_view() {
        let (p, _) = fake(vec![found()]);
        let mut c = LyricsController::new(p, || {});
        c.update(Some(&player("a")), Instant::now());
        c.update(None, Instant::now());
        assert_eq!(c.view().status, LyricsStatus::Idle);
        assert!(c.view().track_key.is_none());
    }
}
