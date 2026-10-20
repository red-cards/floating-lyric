//! Pengatur pengambilan lirik: cache per `track_key`, fetch di thread
//! terpisah, retry otomatis saat error jaringan, dan membuang hasil basi.
//!
//! [`LyricsController`] dipanggil dari thread UI dan tidak pernah blocking:
//! request dikirim ke thread worker lewat channel, hasilnya diambil lagi
//! lewat [`LyricsController::poll`].
//!
//! Kalau lirik yang ketemu tidak cocok, user bisa minta kandidat lain
//! ([`LyricsController::cycle_alternative`]): pencarian lebih luas dijalankan
//! sekali per lagu, lalu user berpindah antar hasil tanpa request baru.

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::lrclib::LrclibClient;
use crate::model::{FetchOutcome, LyricsCandidate, LyricsStatus, PlayerState, TrackQuery};

/// Jeda retry otomatis setelah error jaringan.
pub const RETRY_AFTER: Duration = Duration::from_secs(30);

/// Sumber lirik (LRCLIB di produksi, palsu di test).
pub trait LyricsProvider: Send + 'static {
    fn fetch(&self, query: &TrackQuery) -> FetchOutcome;

    /// Kandidat lirik lain untuk lagu ini, urut dari yang paling cocok.
    /// `Err` = error jaringan. Default: tidak ada alternatif.
    fn alternatives(&self, _query: &TrackQuery) -> Result<Vec<LyricsCandidate>, String> {
        Ok(Vec::new())
    }
}

impl LyricsProvider for LrclibClient {
    fn fetch(&self, query: &TrackQuery) -> FetchOutcome {
        LrclibClient::fetch(self, query)
    }

    fn alternatives(&self, query: &TrackQuery) -> Result<Vec<LyricsCandidate>, String> {
        LrclibClient::alternatives(self, query)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JobKind {
    Fetch,
    Alternatives,
}

struct Job {
    generation: u64,
    kind: JobKind,
    query: TrackQuery,
}

enum JobOutcome {
    Fetch(FetchOutcome),
    Alternatives(Result<Vec<LyricsCandidate>, String>),
}

struct JobResult {
    generation: u64,
    track_key: String,
    outcome: JobOutcome,
    latency: Duration,
}

/// Status kandidat lirik alternatif untuk lagu yang sedang diputar.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum Alternatives {
    /// Belum dicari.
    #[default]
    Unknown,
    Searching,
    /// Pencarian gagal (error jaringan); bisa dicoba lagi.
    Failed(String),
    Ready {
        /// Kandidat yang sedang tampil. `None` = belum memilih (mis. lirik
        /// awal tidak ketemu).
        index: Option<usize>,
        labels: Arc<Vec<String>>,
    },
}

impl Alternatives {
    /// Ada kandidat lain selain yang sedang tampil?
    pub fn has_others(&self) -> bool {
        match self {
            Alternatives::Ready { index, labels } => labels.len() > usize::from(index.is_some()),
            _ => false,
        }
    }

    /// Label kandidat yang sedang tampil.
    pub fn current_label(&self) -> Option<&str> {
        match self {
            Alternatives::Ready {
                index: Some(i),
                labels,
            } => labels.get(*i).map(String::as_str),
            _ => None,
        }
    }
}

/// Kandidat untuk satu lagu + pilihan user (disimpan per `track_key`).
struct AltEntry {
    candidates: Vec<LyricsCandidate>,
    labels: Arc<Vec<String>>,
    index: Option<usize>,
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
    /// Kandidat lirik lain (tombol "Lirik lain").
    pub alternatives: Alternatives,
}

impl Default for LyricsView {
    fn default() -> Self {
        Self {
            track_key: None,
            status: LyricsStatus::Idle,
            latency: None,
            retry_at: None,
            attempts: 0,
            alternatives: Alternatives::Unknown,
        }
    }
}

pub struct LyricsController {
    jobs: Sender<Job>,
    results: Receiver<JobResult>,
    /// Hanya Ready dan NotFound yang di-cache. NetworkError tidak.
    cache: HashMap<String, LyricsStatus>,
    /// Kandidat alternatif per `track_key` (hanya selama app jalan).
    alternatives: HashMap<String, AltEntry>,
    /// Langkah pilihan yang menunggu hasil pencarian alternatif.
    pending_step: Option<i32>,
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
            alternatives: HashMap::new(),
            pending_step: None,
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
            self.pending_step = None;
            self.current_query = Some(player.query());
            self.view = LyricsView {
                track_key: Some(player.track_key.clone()),
                alternatives: self.alternatives.get(&player.track_key).map_or(
                    Alternatives::Unknown,
                    |e| Alternatives::Ready {
                        index: e.index,
                        labels: e.labels.clone(),
                    },
                ),
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
                self.dispatch(JobKind::Fetch);
            }
            return;
        }

        if let (LyricsStatus::NetworkError(_), Some(retry_at)) =
            (&self.view.status, self.view.retry_at)
        {
            if now >= retry_at && !self.in_flight {
                log::info!("retry fetch lirik: {}", player.track_key);
                self.view.retry_at = None;
                self.dispatch(JobKind::Fetch);
            }
        }
    }

    /// Pindah ke kandidat lirik lain (`step` = +1 berikutnya, −1 sebelumnya).
    /// Pertama kali dipanggil untuk sebuah lagu, kandidat dicari dulu di
    /// thread worker; pilihannya diterapkan begitu hasilnya datang.
    pub fn cycle_alternative(&mut self, step: i32) {
        let Some(key) = self.view.track_key.clone() else {
            return;
        };
        if self.in_flight
            || !matches!(
                self.view.status,
                LyricsStatus::Ready(_) | LyricsStatus::NotFound
            )
        {
            return;
        }
        match self.view.alternatives {
            Alternatives::Searching => {}
            Alternatives::Ready { .. } => self.select_alternative(&key, step),
            Alternatives::Unknown | Alternatives::Failed(_) => {
                log::info!("cari lirik alternatif: {key}");
                self.pending_step = Some(step);
                self.view.alternatives = Alternatives::Searching;
                self.dispatch(JobKind::Alternatives);
            }
        }
    }

    fn select_alternative(&mut self, key: &str, step: i32) {
        let Some(entry) = self.alternatives.get_mut(key) else {
            return;
        };
        let n = entry.candidates.len();
        if n == 0 {
            return;
        }
        let next = match entry.index {
            Some(i) => (i as i64 + i64::from(step)).rem_euclid(n as i64) as usize,
            None if step >= 0 => 0,
            None => n - 1,
        };
        if entry.index == Some(next) {
            return;
        }
        entry.index = Some(next);
        let chosen = &entry.candidates[next];
        log::info!("pilih lirik {}/{n}: {}", next + 1, chosen.label);
        let status = LyricsStatus::Ready(Arc::new(chosen.lines.clone()));
        // Pilihan user ikut di-cache, jadi tetap dipakai saat lagu diputar lagi.
        self.cache.insert(key.to_string(), status.clone());
        self.view.status = status;
        self.view.alternatives = Alternatives::Ready {
            index: Some(next),
            labels: entry.labels.clone(),
        };
    }

    fn dispatch(&mut self, kind: JobKind) {
        let Some(query) = self.current_query.clone() else {
            return;
        };
        self.in_flight = true;
        if kind == JobKind::Fetch {
            self.view.attempts += 1;
        }
        let job = Job {
            generation: self.generation,
            kind,
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
        let outcome = match res.outcome {
            JobOutcome::Fetch(o) => o,
            JobOutcome::Alternatives(result) => {
                self.apply_alternatives(res.track_key, result);
                return;
            }
        };
        self.view.latency = Some(res.latency);
        match outcome {
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

    fn apply_alternatives(&mut self, key: String, result: Result<Vec<LyricsCandidate>, String>) {
        let step = self.pending_step.take();
        let mut candidates = match result {
            Ok(c) => c,
            Err(e) => {
                log::warn!("cari lirik alternatif gagal: {e}");
                self.view.alternatives = Alternatives::Failed(e);
                return;
            }
        };
        // Lirik yang sedang tampil selalu jadi salah satu pilihan, supaya
        // user bisa kembali ke hasil awal.
        let index = match &self.view.status {
            LyricsStatus::Ready(lines) => {
                Some(match candidates.iter().position(|c| c.lines == **lines) {
                    Some(i) => i,
                    None => {
                        candidates.insert(
                            0,
                            LyricsCandidate {
                                id: None,
                                label: "Hasil awal".into(),
                                lines: lines.as_ref().clone(),
                            },
                        );
                        0
                    }
                })
            }
            _ => None,
        };
        log::info!("{} kandidat lirik untuk {key}", candidates.len());
        let labels = Arc::new(candidates.iter().map(|c| c.label.clone()).collect());
        self.view.alternatives = Alternatives::Ready {
            index,
            labels: Arc::clone(&labels),
        };
        self.alternatives.insert(
            key.clone(),
            AltEntry {
                candidates,
                labels,
                index,
            },
        );
        if let Some(step) = step {
            self.select_alternative(&key, step);
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
        if job.kind == JobKind::Alternatives {
            let started = Instant::now();
            let found =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| provider.alternatives(q)))
                    .unwrap_or_else(|_| Err("panic saat mencari lirik alternatif".into()));
            let res = JobResult {
                generation: job.generation,
                track_key: job.query.track_key,
                outcome: JobOutcome::Alternatives(found),
                latency: started.elapsed(),
            };
            if results.send(res).is_err() {
                break;
            }
            on_result();
            continue;
        }
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
            outcome: JobOutcome::Fetch(outcome),
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

    type AltsFn = fn() -> Result<Vec<LyricsCandidate>, String>;

    /// Provider palsu: hasil diambil berurutan dari daftar, dan bisa ditahan
    /// dengan gate supaya kita bisa mensimulasikan request yang masih jalan.
    struct Fake {
        outcomes: Arc<Mutex<Vec<FetchOutcome>>>,
        calls: Arc<AtomicUsize>,
        gate: Option<Arc<Mutex<Receiver<()>>>>,
        /// Kalau diisi, hasil ditentukan dari track_key, bukan urutan panggilan.
        by_key: Option<fn(&str) -> FetchOutcome>,
        alts: Option<AltsFn>,
        alt_calls: Arc<AtomicUsize>,
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

        fn alternatives(&self, _q: &TrackQuery) -> Result<Vec<LyricsCandidate>, String> {
            self.alt_calls.fetch_add(1, Ordering::SeqCst);
            self.alts.map_or(Ok(Vec::new()), |f| f())
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
                alts: None,
                alt_calls: Arc::new(AtomicUsize::new(0)),
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

    fn lines(text: &str) -> Vec<LyricLine> {
        vec![LyricLine {
            time_secs: 1.0,
            text: text.into(),
        }]
    }

    fn found() -> FetchOutcome {
        FetchOutcome::Found(lines("x"))
    }

    fn candidate(text: &str) -> LyricsCandidate {
        LyricsCandidate {
            id: None,
            label: format!("kandidat {text}"),
            lines: lines(text),
        }
    }

    fn shown_text(c: &LyricsController) -> Option<String> {
        match &c.view().status {
            LyricsStatus::Ready(l) => Some(l[0].text.clone()),
            _ => None,
        }
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
    fn cycles_alternatives_with_one_search() {
        let (mut p, _) = fake(vec![found()]);
        p.alts = Some(|| Ok(vec![candidate("x"), candidate("y"), candidate("z")]));
        let alt_calls = p.alt_calls.clone();
        let mut c = LyricsController::new(p, || {});
        let a = player("a");
        c.update(Some(&a), Instant::now());
        assert!(c.poll_blocking(WAIT));

        c.cycle_alternative(1);
        assert_eq!(c.view().alternatives, Alternatives::Searching);
        assert!(c.poll_blocking(WAIT));
        // Lirik awal ("x") dikenali sebagai kandidat pertama → pindah ke "y".
        assert_eq!(shown_text(&c).as_deref(), Some("y"));
        assert_eq!(c.view().alternatives.current_label(), Some("kandidat y"));

        c.cycle_alternative(1);
        c.cycle_alternative(1);
        assert_eq!(shown_text(&c).as_deref(), Some("x"));
        c.cycle_alternative(-1);
        assert_eq!(shown_text(&c).as_deref(), Some("z"));
        assert_eq!(alt_calls.load(Ordering::SeqCst), 1);

        // Pindah lagu lalu kembali: pilihan user tetap dipakai.
        c.update(Some(&player("b")), Instant::now());
        assert!(c.poll_blocking(WAIT));
        c.update(Some(&a), Instant::now());
        assert_eq!(shown_text(&c).as_deref(), Some("z"));
        assert!(c.view().alternatives.has_others());
    }

    #[test]
    fn original_lyrics_stay_selectable() {
        let (mut p, _) = fake(vec![found()]);
        p.alts = Some(|| Ok(vec![candidate("y")]));
        let mut c = LyricsController::new(p, || {});
        c.update(Some(&player("a")), Instant::now());
        assert!(c.poll_blocking(WAIT));
        c.cycle_alternative(1);
        assert!(c.poll_blocking(WAIT));
        assert_eq!(shown_text(&c).as_deref(), Some("y"));
        c.cycle_alternative(1);
        assert_eq!(shown_text(&c).as_deref(), Some("x"));
        assert_eq!(c.view().alternatives.current_label(), Some("Hasil awal"));
    }

    #[test]
    fn alternatives_rescue_not_found() {
        let (mut p, _) = fake(vec![FetchOutcome::NotFound]);
        p.alts = Some(|| Ok(vec![candidate("y")]));
        let mut c = LyricsController::new(p, || {});
        c.update(Some(&player("a")), Instant::now());
        assert!(c.poll_blocking(WAIT));
        assert_eq!(c.view().status, LyricsStatus::NotFound);
        c.cycle_alternative(1);
        assert!(c.poll_blocking(WAIT));
        assert_eq!(shown_text(&c).as_deref(), Some("y"));
        // Satu-satunya kandidat sudah tampil.
        assert!(!c.view().alternatives.has_others());
    }

    #[test]
    fn failed_alternative_search_can_be_retried() {
        let (mut p, _) = fake(vec![found()]);
        p.alts = Some(|| Err("timeout".into()));
        let alt_calls = p.alt_calls.clone();
        let mut c = LyricsController::new(p, || {});
        c.update(Some(&player("a")), Instant::now());
        assert!(c.poll_blocking(WAIT));
        c.cycle_alternative(1);
        assert!(c.poll_blocking(WAIT));
        assert!(matches!(c.view().alternatives, Alternatives::Failed(_)));
        assert_eq!(shown_text(&c).as_deref(), Some("x"));
        c.cycle_alternative(1);
        assert!(c.poll_blocking(WAIT));
        assert_eq!(alt_calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn no_alternatives_while_loading() {
        let (mut p, _) = fake(vec![found()]);
        p.alts = Some(|| Ok(vec![candidate("y")]));
        let alt_calls = p.alt_calls.clone();
        let mut c = LyricsController::new(p, || {});
        c.update(Some(&player("a")), Instant::now());
        c.cycle_alternative(1);
        assert_eq!(c.view().alternatives, Alternatives::Unknown);
        assert!(c.poll_blocking(WAIT));
        assert_eq!(alt_calls.load(Ordering::SeqCst), 0);
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
