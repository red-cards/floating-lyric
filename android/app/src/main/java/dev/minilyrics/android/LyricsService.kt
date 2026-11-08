package dev.minilyrics.android

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Intent
import android.content.SharedPreferences
import android.content.pm.PackageManager
import android.content.pm.ServiceInfo
import android.graphics.Bitmap
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.support.v4.media.MediaBrowserCompat.MediaItem
import android.support.v4.media.MediaDescriptionCompat
import android.support.v4.media.MediaMetadataCompat
import android.support.v4.media.session.MediaSessionCompat
import android.support.v4.media.session.PlaybackStateCompat
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat
import androidx.media.MediaBrowserServiceCompat
import androidx.media.session.MediaButtonReceiver
import dev.minilyrics.android.lyrics.Lrc
import dev.minilyrics.android.lyrics.LyricsRepository
import dev.minilyrics.android.lyrics.LyricsResult
import dev.minilyrics.android.player.Availability
import dev.minilyrics.android.player.DemoSource
import dev.minilyrics.android.player.Playback
import dev.minilyrics.android.player.PlayerSource
import dev.minilyrics.android.player.SpotifySource
import dev.minilyrics.android.player.Track
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch

/**
 * "Proxy media app" untuk Android Auto.
 *
 * Tidak memutar audio. Session-nya mencerminkan Spotify (status, posisi,
 * durasi, artwork), tapi title/subtitle diisi baris lirik aktif. Semua tombol
 * transport diteruskan ke Spotify.
 */
class LyricsService : MediaBrowserServiceCompat() {

    private lateinit var session: MediaSessionCompat
    private lateinit var repository: LyricsRepository
    private lateinit var source: PlayerSource

    private val handler = Handler(Looper.getMainLooper())
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)

    /** `result == null` artinya sedang dicari. */
    private data class LyricsState(val trackKey: String, val result: LyricsResult?, val source: String)

    private var lyrics: LyricsState? = null
    private var fetchJob: Job? = null
    private var lastCard: CardText.Card? = null
    private var artKey: Any? = null
    private var art: Bitmap? = null
    private var isForeground = false

    // Pengaman auto-resume (lihat onSourcePlayback).
    private var playCommandAt = 0L
    private var userPauseAt = 0L
    private var autoResumeUsed = true
    private var lastSourceState: Int? = null
    private var autoResumeCount = 0

    private var lastRootAt = 0L

    private val lineTick = Runnable { refreshCard() }
    private val retryLyrics = Runnable { source.track?.let { startLookup(it, force = true) } }

    private val prefsListener = SharedPreferences.OnSharedPreferenceChangeListener { _, key ->
        if (key == Prefs.KEY_DEMO) switchSource()
    }

    // ------------------------------------------------------------ lifecycle

    override fun onCreate() {
        super.onCreate()
        Log.i(TAG, "service onCreate (versi ${BuildConfig.VERSION_NAME})")
        createChannel()
        repository = LyricsRepository(this, BuildConfig.VERSION_NAME)

        session = MediaSessionCompat(this, "MiniLyricsSession").apply {
            setCallback(sessionCallback)
            setSessionActivity(openAppIntent())
            // Android Auto selalu menyediakan slot untuk prev/next lagu (bukan
            // diisi tombol lain) — lihat README bagian tombol.
            setExtras(
                Bundle().apply {
                    putBoolean(SLOT_RESERVATION_SKIP_TO_NEXT, true)
                    putBoolean(SLOT_RESERVATION_SKIP_TO_PREV, true)
                }
            )
            isActive = true
        }
        sessionToken = session.sessionToken

        source = makeSource()
        source.start(sourceListener)
        Prefs.prefs(this).registerOnSharedPreferenceChangeListener(prefsListener)
        publishPlaybackState()
        refreshCard(force = true)
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        Log.i(TAG, "onStartCommand action=${intent?.action}")
        MediaButtonReceiver.handleIntent(session, intent)
        return START_NOT_STICKY
    }

    override fun onDestroy() {
        Log.i(TAG, "service onDestroy")
        Prefs.prefs(this).unregisterOnSharedPreferenceChangeListener(prefsListener)
        handler.removeCallbacksAndMessages(null)
        source.stop()
        scope.cancel()
        session.isActive = false
        session.release()
        super.onDestroy()
    }

    private fun makeSource(): PlayerSource =
        if (Prefs.demoEnabled(this)) DemoSource(handler) else SpotifySource(this, handler)

    private fun switchSource() {
        val demo = Prefs.demoEnabled(this)
        if ((source is DemoSource) == demo) return
        Log.i(TAG, "ganti sumber → ${if (demo) "Demo" else "Spotify"}")
        source.stop()
        fetchJob?.cancel()
        handler.removeCallbacks(retryLyrics)
        lyrics = null
        lastSourceState = null
        source = makeSource()
        source.start(sourceListener)
        notifyBrowseTreeChanged()
        publishPlaybackState()
        refreshCard(force = true)
    }

    private fun notifyBrowseTreeChanged() = notifyChildrenChanged(ROOT_ID)

    // ------------------------------------------------------------ browse tree

    override fun onGetRoot(clientPackageName: String, clientUid: Int, rootHints: Bundle?): BrowserRoot {
        lastRootAt = SystemClock.elapsedRealtime()
        Log.i(TAG, "onGetRoot dari $clientPackageName (uid=$clientUid)")
        return BrowserRoot(ROOT_ID, null)
    }

    override fun onLoadChildren(parentId: String, result: Result<MutableList<MediaItem>>) {
        Log.i(TAG, "onLoadChildren parentId=$parentId")
        if (parentId != ROOT_ID) {
            result.sendResult(mutableListOf())
            return
        }
        val desc = MediaDescriptionCompat.Builder()
            .setMediaId(MEDIA_ID)
            .setTitle("Tampilkan lirik (${source.name})")
            .setSubtitle("Tombol diteruskan ke ${source.name}")
            .build()
        result.sendResult(mutableListOf(MediaItem(desc, MediaItem.FLAG_PLAYABLE)))
    }

    // ------------------------------------------------------------ transport (dari card / AA)

    private val sessionCallback = object : MediaSessionCompat.Callback() {
        override fun onPlay() = playRequested("onPlay")

        override fun onPlayFromMediaId(mediaId: String?, extras: Bundle?) =
            playRequested("onPlayFromMediaId($mediaId)")

        override fun onPlayFromSearch(query: String?, extras: Bundle?) =
            playRequested("onPlayFromSearch(\"$query\")")

        override fun onPrepare() {
            Log.i(TAG, "onPrepare (diabaikan, tidak auto-play)")
        }

        override fun onPause() {
            Log.i(TAG, "onPause → pause ${source.name}")
            userPauseAt = SystemClock.elapsedRealtime()
            source.pause()
        }

        override fun onStop() {
            Log.i(TAG, "onStop → pause ${source.name}")
            userPauseAt = SystemClock.elapsedRealtime()
            source.pause()
        }

        override fun onSkipToNext() {
            Log.i(TAG, "onSkipToNext → ${source.name}")
            source.skipToNext()
        }

        override fun onSkipToPrevious() {
            Log.i(TAG, "onSkipToPrevious → ${source.name}")
            source.skipToPrevious()
        }

        override fun onSeekTo(pos: Long) {
            Log.i(TAG, "onSeekTo $pos → ${source.name}")
            source.seekTo(pos)
        }

        // Tidak diiklankan di actions. Dicatat supaya ketahuan kalau Android
        // Auto tetap mengirimnya (tombol replay-10/forward-10).
        override fun onRewind() {
            Log.w(TAG, "onRewind diterima (tidak diiklankan) → seek -10 dtk")
            seekBy(-10_000)
        }

        override fun onFastForward() {
            Log.w(TAG, "onFastForward diterima (tidak diiklankan) → seek +10 dtk")
            seekBy(10_000)
        }

        override fun onCustomAction(action: String?, extras: Bundle?) {
            Log.w(TAG, "onCustomAction $action extras=${extras?.keySet()} (diabaikan)")
        }

        override fun onMediaButtonEvent(mediaButtonEvent: Intent?): Boolean {
            Log.i(TAG, "onMediaButtonEvent $mediaButtonEvent")
            return super.onMediaButtonEvent(mediaButtonEvent)
        }
    }

    private fun playRequested(what: String) {
        playCommandAt = SystemClock.elapsedRealtime()
        autoResumeUsed = false
        Log.i(TAG, "$what → play ${source.name} (state sumber=${source.playback?.state?.let(Playback::stateName)})")
        source.play()
    }

    private fun seekBy(deltaMs: Long) {
        val t = source.track
        val pos = source.playback?.positionAt(SystemClock.elapsedRealtime(), t?.durationMs ?: 0) ?: return
        source.seekTo((pos + deltaMs).coerceAtLeast(0))
    }

    // ------------------------------------------------------------ sumber (Spotify / demo)

    private val sourceListener = object : PlayerSource.Listener {
        override fun onAvailabilityChanged(availability: Availability) {
            Log.i(TAG, "${source.name}: availability=$availability")
            publishPlaybackState()
            refreshCard()
        }

        override fun onTrackChanged(track: Track?) {
            if (track?.key != lyrics?.trackKey) {
                Log.i(TAG, "lagu: ${track?.let { "\"${it.title}\" — ${it.artist} [${it.album}] ${it.durationMs / 1000}s" } ?: "-"}")
                if (track != null) startLookup(track) else {
                    fetchJob?.cancel()
                    handler.removeCallbacks(retryLyrics)
                    lyrics = null
                }
            }
            updateArt(track)
            publishPlaybackState()
            refreshCard(force = true)
        }

        override fun onPlaybackChanged(playback: Playback?) = onSourcePlayback(playback)
    }

    private fun onSourcePlayback(pb: Playback?) {
        val now = SystemClock.elapsedRealtime()
        val prev = lastSourceState
        lastSourceState = pb?.state
        if (pb?.state != prev) {
            Log.i(
                TAG,
                "${source.name} state ${prev?.let(Playback::stateName) ?: "-"} → " +
                    "${pb?.state?.let(Playback::stateName) ?: "-"} pos=${pb?.positionMs} ms" +
                    (if (lastRootAt > 0) " (onGetRoot terakhir ${now - lastRootAt} ms lalu)" else ""),
            )
        }

        // Pengaman: kalau dalam GUARD_MS setelah MiniLyrics di-play, sumber
        // berubah ke PAUSED tanpa ada perintah pause dari user, kirim play sekali.
        if (pb != null && pb.state == STATE_PAUSED && prev != STATE_PAUSED) {
            val sincePlay = now - playCommandAt
            val userPausedAfterPlay = userPauseAt >= playCommandAt
            if (playCommandAt > 0 && sincePlay <= GUARD_MS && !userPausedAfterPlay && !autoResumeUsed) {
                autoResumeUsed = true
                autoResumeCount++
                val msg = "${source.name} ter-pause $sincePlay ms setelah MiniLyrics di-play, tanpa perintah pause dari user"
                Log.w(TAG, "AUTO-RESUME: $msg → kirim play ke ${source.name} (sekali)")
                StatusBus.update {
                    it.copy(
                        autoResumeCount = autoResumeCount,
                        lastAutoResume = "${java.text.DateFormat.getTimeInstance().format(java.util.Date())}: $msg",
                    )
                }
                source.play()
            } else if (playCommandAt > 0 && sincePlay <= GUARD_MS) {
                Log.i(TAG, "AUTO-RESUME dilewati (userPause=$userPausedAfterPlay, sudah dipakai=$autoResumeUsed)")
            }
        }

        publishPlaybackState()
        refreshCard()
        updateForeground()
    }

    // ------------------------------------------------------------ lirik

    private fun startLookup(track: Track, force: Boolean = false) {
        if (!force && lyrics?.trackKey == track.key) return
        fetchJob?.cancel()
        handler.removeCallbacks(retryLyrics)
        source.embeddedLyrics?.let {
            lyrics = LyricsState(track.key, LyricsResult.Synced(it), "demo")
            pushStatus()
            return
        }
        lyrics = LyricsState(track.key, null, "")
        pushStatus()
        fetchJob = scope.launch {
            val found = repository.lookup(track.query())
            if (source.track?.key != track.key) return@launch // lagu sudah ganti
            Log.i(TAG, "lirik \"${track.title}\": ${LyricsRepository.describe(found.result)} dari ${found.source}")
            lyrics = LyricsState(track.key, found.result, found.source)
            if (found.result is LyricsResult.NetworkError) handler.postDelayed(retryLyrics, RETRY_LYRICS_MS)
            refreshCard()
        }
    }

    // ------------------------------------------------------------ card

    /**
     * Hitung teks card dari posisi sekarang, publish kalau berubah, lalu jadwalkan
     * update berikutnya tepat di timestamp baris berikutnya (bukan tick berkala).
     */
    private fun refreshCard(force: Boolean = false) {
        handler.removeCallbacks(lineTick)
        val track = source.track
        val pb = source.playback
        val pos = pb?.positionAt(SystemClock.elapsedRealtime(), track?.durationMs ?: 0) ?: 0
        val (card, nextChangeMs) = computeCard(track, pos)

        if (force || card != lastCard) {
            lastCard = card
            publishMetadata(track, card)
            Log.d(TAG, "card @${pos} ms: \"${card.title}\" / \"${card.subtitle}\"")
            if (isForeground) postNotification()
        }
        pushStatus()

        if (pb != null && pb.isPlaying && nextChangeMs != null && nextChangeMs > pos) {
            val delay = ((nextChangeMs - pos) / pb.effectiveSpeed).toLong() + LINE_TICK_SLACK_MS
            handler.postDelayed(lineTick, delay)
        }
    }

    /** Card + posisi (ms) saat card berikutnya berubah, atau null. */
    private fun computeCard(track: Track?, pos: Long): Pair<CardText.Card, Long?> {
        when (source.availability) {
            Availability.NO_PERMISSION -> return CardText.Card("Izin belum aktif", "Buka app MiniLyrics") to null
            Availability.NOT_RUNNING -> return CardText.Card("${source.name} tidak aktif", "Putar lagu dulu") to null
            Availability.AVAILABLE -> Unit
        }
        if (track == null) return CardText.Card("MiniLyrics", "Belum ada lagu") to null
        val state = lyrics?.takeIf { it.trackKey == track.key }
        val result = state?.result ?: return CardText.Card(track.title, "mencari lirik…") to null
        return when (result) {
            is LyricsResult.Synced -> {
                val lines = result.lines
                val idx = Lrc.activeIndex(lines, pos)
                if (idx < 0) {
                    // Intro: judul lagu, dengan cuplikan baris pertama.
                    CardText.forLine(track.title, lines.first().text) to lines.first().timeMs
                } else {
                    CardText.forLine(lines[idx].text, lines.getOrNull(idx + 1)?.text) to lines.getOrNull(idx + 1)?.timeMs
                }
            }
            LyricsResult.Unsynced -> CardText.Card(track.title, "lirik tidak sinkron") to null
            LyricsResult.Instrumental -> CardText.Card(track.title, "♪ instrumental") to null
            LyricsResult.NotFound, is LyricsResult.NetworkError -> CardText.Card(track.title, track.artist) to null
        }
    }

    private fun publishMetadata(track: Track?, card: CardText.Card) {
        val b = MediaMetadataCompat.Builder()
            .putString(MediaMetadataCompat.METADATA_KEY_MEDIA_ID, track?.key ?: MEDIA_ID)
            .putString(MediaMetadataCompat.METADATA_KEY_TITLE, card.title)
            .putString(MediaMetadataCompat.METADATA_KEY_DISPLAY_TITLE, card.title)
            .putString(MediaMetadataCompat.METADATA_KEY_ARTIST, card.subtitle)
            .putString(MediaMetadataCompat.METADATA_KEY_DISPLAY_SUBTITLE, card.subtitle)
        if (track != null) {
            b.putString(MediaMetadataCompat.METADATA_KEY_ALBUM, "${track.title} — ${track.artist}")
            if (track.durationMs > 0) b.putLong(MediaMetadataCompat.METADATA_KEY_DURATION, track.durationMs)
            art?.let {
                b.putBitmap(MediaMetadataCompat.METADATA_KEY_ALBUM_ART, it)
                b.putBitmap(MediaMetadataCompat.METADATA_KEY_DISPLAY_ICON, it)
            }
        }
        session.setMetadata(b.build())
    }

    /** Album art Spotify, diperkecil sekali per lagu supaya murah dikirim tiap ganti baris. */
    private fun updateArt(track: Track?) {
        val original = track?.art
        val key = original?.let { System.identityHashCode(it) }
        if (key == artKey) return
        artKey = key
        art = original?.let {
            val longest = maxOf(it.width, it.height)
            if (longest <= ART_MAX_PX) it else {
                val scale = ART_MAX_PX.toFloat() / longest
                Bitmap.createScaledBitmap(it, (it.width * scale).toInt().coerceAtLeast(1), (it.height * scale).toInt().coerceAtLeast(1), true)
            }
        }
    }

    /** PlaybackState mengikuti sumber: status, posisi, kecepatan, waktu update. */
    private fun publishPlaybackState() {
        val pb = source.playback
        val usable = source.availability == Availability.AVAILABLE && source.track != null && pb != null
        val state = when {
            !usable -> PlaybackStateCompat.STATE_PAUSED
            pb!!.isPlaying -> PlaybackStateCompat.STATE_PLAYING
            pb.state == STATE_BUFFERING || pb.state == STATE_CONNECTING ||
                pb.state == STATE_SKIPPING_TO_NEXT || pb.state == STATE_SKIPPING_TO_PREVIOUS ->
                PlaybackStateCompat.STATE_BUFFERING
            else -> PlaybackStateCompat.STATE_PAUSED
        }
        val builder = PlaybackStateCompat.Builder().setActions(ACTIONS)
        if (usable) {
            builder.setState(state, pb!!.positionMs, if (pb.isPlaying) pb.effectiveSpeed else 0f, pb.updateTimeMs)
        } else {
            builder.setState(state, 0, 0f, SystemClock.elapsedRealtime())
        }
        session.setPlaybackState(builder.build())
    }

    private fun pushStatus() {
        val t = source.track
        val l = lyrics?.takeIf { it.trackKey == t?.key }
        val lyricsLabel = when {
            t == null -> "-"
            l == null || l.result == null -> "mencari…"
            else -> "${LyricsRepository.describe(l.result)} · ${l.source}"
        }
        StatusBus.update {
            it.copy(
                sourceName = source.name,
                availability = source.availability,
                playerState = source.playback?.state?.let(Playback::stateName) ?: "-",
                trackLabel = t?.let { tr -> "${tr.title} — ${tr.artist}" },
                lyricsLabel = lyricsLabel,
                cardTitle = lastCard?.title.orEmpty(),
                cardSubtitle = lastCard?.subtitle.orEmpty(),
            )
        }
    }

    // ------------------------------------------------------------ foreground & notifikasi

    private fun updateForeground() {
        val playing = source.playback?.isPlaying == true
        if (playing && !isForeground) {
            goForeground()
        } else if (!playing && isForeground) {
            ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_DETACH)
            isForeground = false
            postNotification()
        }
    }

    private fun goForeground() {
        try {
            ServiceCompat.startForeground(
                this, NOTIFICATION_ID, buildNotification(),
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK else 0,
            )
            isForeground = true
            Log.i(TAG, "startForeground OK")
        } catch (e: Exception) {
            // Android 12+/14 bisa menolak kalau dipicu dari background.
            Log.w(TAG, "startForeground gagal: ${e.javaClass.simpleName}: ${e.message}")
            return
        }
        try {
            startService(Intent(this, LyricsService::class.java))
        } catch (e: Exception) {
            Log.w(TAG, "startService gagal: ${e.javaClass.simpleName}: ${e.message}")
        }
    }

    private fun postNotification() {
        if (Build.VERSION.SDK_INT >= 33 &&
            ContextCompat.checkSelfPermission(this, android.Manifest.permission.POST_NOTIFICATIONS) !=
            PackageManager.PERMISSION_GRANTED
        ) {
            return
        }
        NotificationManagerCompat.from(this).notify(NOTIFICATION_ID, buildNotification())
    }

    private fun buildNotification(): Notification {
        val playing = source.playback?.isPlaying == true
        val card = lastCard
        val playPause = if (playing) {
            NotificationCompat.Action(
                android.R.drawable.ic_media_pause, "Pause",
                MediaButtonReceiver.buildMediaButtonPendingIntent(this, PlaybackStateCompat.ACTION_PAUSE),
            )
        } else {
            NotificationCompat.Action(
                android.R.drawable.ic_media_play, "Play",
                MediaButtonReceiver.buildMediaButtonPendingIntent(this, PlaybackStateCompat.ACTION_PLAY),
            )
        }
        return NotificationCompat.Builder(this, CHANNEL_ID)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(card?.title)
            .setContentText(card?.subtitle)
            .setSubText("MiniLyrics")
            .setLargeIcon(art)
            .setContentIntent(openAppIntent())
            .setVisibility(NotificationCompat.VISIBILITY_PUBLIC)
            .setOnlyAlertOnce(true)
            .setSilent(true)
            .setOngoing(playing)
            .addAction(
                android.R.drawable.ic_media_previous, "Sebelumnya",
                MediaButtonReceiver.buildMediaButtonPendingIntent(this, PlaybackStateCompat.ACTION_SKIP_TO_PREVIOUS),
            )
            .addAction(playPause)
            .addAction(
                android.R.drawable.ic_media_next, "Berikutnya",
                MediaButtonReceiver.buildMediaButtonPendingIntent(this, PlaybackStateCompat.ACTION_SKIP_TO_NEXT),
            )
            .setStyle(
                androidx.media.app.NotificationCompat.MediaStyle()
                    .setMediaSession(session.sessionToken)
                    .setShowActionsInCompactView(0, 1, 2)
            )
            .build()
    }

    private fun createChannel() {
        val channel = NotificationChannel(CHANNEL_ID, getString(R.string.channel_name), NotificationManager.IMPORTANCE_LOW)
        getSystemService(NotificationManager::class.java).createNotificationChannel(channel)
    }

    private fun openAppIntent(): PendingIntent = PendingIntent.getActivity(
        this, 0,
        Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
    )

    companion object {
        const val TAG = "MiniLyrics"

        private const val ROOT_ID = "root"
        private const val MEDIA_ID = "minilyrics"
        private const val CHANNEL_ID = "lyrics"
        private const val NOTIFICATION_ID = 1

        /** Jendela pengaman auto-resume setelah MiniLyrics di-play. */
        private const val GUARD_MS = 3_000L
        private const val RETRY_LYRICS_MS = 30_000L
        /** Sedikit telat dari timestamp baris supaya indeks sudah pasti berganti. */
        private const val LINE_TICK_SLACK_MS = 20L
        private const val ART_MAX_PX = 320

        private const val STATE_PAUSED = android.media.session.PlaybackState.STATE_PAUSED
        private const val STATE_BUFFERING = android.media.session.PlaybackState.STATE_BUFFERING
        private const val STATE_CONNECTING = android.media.session.PlaybackState.STATE_CONNECTING
        private const val STATE_SKIPPING_TO_NEXT = android.media.session.PlaybackState.STATE_SKIPPING_TO_NEXT
        private const val STATE_SKIPPING_TO_PREVIOUS = android.media.session.PlaybackState.STATE_SKIPPING_TO_PREVIOUS

        // androidx.media3.session.legacy.MediaConstants.SESSION_EXTRAS_KEY_SLOT_RESERVATION_*
        private const val SLOT_RESERVATION_SKIP_TO_NEXT = "android.media.playback.ALWAYS_RESERVE_SPACE_FOR.ACTION_SKIP_TO_NEXT"
        private const val SLOT_RESERVATION_SKIP_TO_PREV = "android.media.playback.ALWAYS_RESERVE_SPACE_FOR.ACTION_SKIP_TO_PREVIOUS"

        /**
         * Sengaja TANPA ACTION_REWIND / ACTION_FAST_FORWARD dan tanpa custom
         * action, supaya slot kiri/kanan di card hanya bisa diisi prev/next lagu.
         */
        private const val ACTIONS =
            PlaybackStateCompat.ACTION_PLAY or
                PlaybackStateCompat.ACTION_PAUSE or
                PlaybackStateCompat.ACTION_PLAY_PAUSE or
                PlaybackStateCompat.ACTION_STOP or
                PlaybackStateCompat.ACTION_SKIP_TO_NEXT or
                PlaybackStateCompat.ACTION_SKIP_TO_PREVIOUS or
                PlaybackStateCompat.ACTION_SEEK_TO or
                PlaybackStateCompat.ACTION_PLAY_FROM_MEDIA_ID or
                PlaybackStateCompat.ACTION_PLAY_FROM_SEARCH
    }
}
