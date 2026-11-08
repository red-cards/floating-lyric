package dev.minilyrics.android.player

import android.content.ComponentName
import android.content.Context
import android.graphics.Bitmap
import android.media.MediaMetadata
import android.media.session.MediaController
import android.media.session.MediaSessionManager
import android.media.session.PlaybackState
import android.os.Handler
import android.util.Log
import androidx.core.app.NotificationManagerCompat

/**
 * Membaca dan mengendalikan session media Spotify lewat
 * `MediaSessionManager.getActiveSessions()`. Izinnya datang dari
 * [SpotifyNotificationListener] (notification listener yang aktif), bukan dari
 * isi notifikasinya.
 */
class SpotifySource(
    private val context: Context,
    private val handler: Handler,
) : PlayerSource {

    override val name = "Spotify"
    override var availability = Availability.NOT_RUNNING
        private set
    override var track: Track? = null
        private set
    override var playback: Playback? = null
        private set

    private val msm = context.getSystemService(MediaSessionManager::class.java)
    private val listenerComponent = ComponentName(context, SpotifyNotificationListener::class.java)
    private var listener: PlayerSource.Listener? = null
    private var controller: MediaController? = null
    private var sessionsListenerRegistered = false

    private val retryAttach = Runnable { attach() }

    private val sessionsListener = MediaSessionManager.OnActiveSessionsChangedListener { controllers ->
        Log.d(TAG, "onSessionsChanged: ${controllers?.joinToString { it.packageName }}")
        pickController(controllers)
    }

    private val controllerCallback = object : MediaController.Callback() {
        override fun onMetadataChanged(metadata: MediaMetadata?) = updateTrack(metadata)
        override fun onPlaybackStateChanged(state: PlaybackState?) = updatePlayback(state)
        override fun onSessionDestroyed() {
            Log.i(TAG, "session Spotify dihancurkan")
            setController(null)
        }
    }

    override fun start(listener: PlayerSource.Listener) {
        this.listener = listener
        attach()
    }

    override fun stop() {
        handler.removeCallbacks(retryAttach)
        if (sessionsListenerRegistered) {
            msm.removeOnActiveSessionsChangedListener(sessionsListener)
            sessionsListenerRegistered = false
        }
        setController(null)
        listener = null
    }

    /** Coba pasang listener session; diulang tiap 5 dtk selama izin belum ada. */
    private fun attach() {
        handler.removeCallbacks(retryAttach)
        if (!hasListenerPermission(context)) {
            setAvailability(Availability.NO_PERMISSION)
            handler.postDelayed(retryAttach, RETRY_MS)
            return
        }
        try {
            if (!sessionsListenerRegistered) {
                msm.addOnActiveSessionsChangedListener(sessionsListener, listenerComponent, handler)
                sessionsListenerRegistered = true
                Log.i(TAG, "listener session aktif terpasang")
            }
            pickController(msm.getActiveSessions(listenerComponent))
        } catch (e: SecurityException) {
            // Izin baru dicabut, atau listener belum ter-bind oleh sistem.
            Log.w(TAG, "getActiveSessions ditolak: ${e.message}")
            sessionsListenerRegistered = false
            setAvailability(Availability.NO_PERMISSION)
            handler.postDelayed(retryAttach, RETRY_MS)
        }
    }

    private fun pickController(controllers: List<MediaController>?) {
        val spotify = controllers?.firstOrNull {
            // Session milik app sendiri selalu diabaikan.
            it.packageName == SPOTIFY_PACKAGE && it.packageName != context.packageName
        }
        if (spotify?.sessionToken == controller?.sessionToken) {
            // Session sama (atau sama-sama tidak ada): cukup perbarui status.
            if (spotify == null) setAvailability(Availability.NOT_RUNNING)
            return
        }
        setController(spotify)
    }

    private fun setController(c: MediaController?) {
        controller?.unregisterCallback(controllerCallback)
        controller = c
        if (c == null) {
            Log.i(TAG, "session Spotify tidak ditemukan")
            setAvailability(if (hasListenerPermission(context)) Availability.NOT_RUNNING else Availability.NO_PERMISSION)
            track = null
            playback = null
            listener?.onTrackChanged(null)
            listener?.onPlaybackChanged(null)
            return
        }
        Log.i(TAG, "terhubung ke session Spotify")
        c.registerCallback(controllerCallback, handler)
        setAvailability(Availability.AVAILABLE)
        updateTrack(c.metadata)
        updatePlayback(c.playbackState)
    }

    private fun setAvailability(a: Availability) {
        if (a == availability) return
        availability = a
        listener?.onAvailabilityChanged(a)
    }

    private fun updateTrack(m: MediaMetadata?) {
        val t = m?.let {
            Track(
                title = it.getString(MediaMetadata.METADATA_KEY_TITLE).orEmpty(),
                artist = it.getString(MediaMetadata.METADATA_KEY_ARTIST)
                    ?: it.getString(MediaMetadata.METADATA_KEY_ALBUM_ARTIST).orEmpty(),
                album = it.getString(MediaMetadata.METADATA_KEY_ALBUM).orEmpty(),
                durationMs = it.getLong(MediaMetadata.METADATA_KEY_DURATION),
                art = artOf(it),
            )
        }?.takeIf { it.title.isNotBlank() }
        track = t
        listener?.onTrackChanged(t)
    }

    private fun updatePlayback(s: PlaybackState?) {
        val p = s?.let { Playback(it.state, it.position, it.lastPositionUpdateTime, it.playbackSpeed) }
        playback = p
        listener?.onPlaybackChanged(p)
    }

    private fun artOf(m: MediaMetadata): Bitmap? =
        m.getBitmap(MediaMetadata.METADATA_KEY_ALBUM_ART)
            ?: m.getBitmap(MediaMetadata.METADATA_KEY_ART)
            ?: m.getBitmap(MediaMetadata.METADATA_KEY_DISPLAY_ICON)

    private fun controls() = controller?.transportControls.also {
        if (it == null) Log.w(TAG, "perintah diabaikan: Spotify tidak terhubung")
    }

    override fun play() { controls()?.play() }
    override fun pause() { controls()?.pause() }
    override fun skipToNext() { controls()?.skipToNext() }
    override fun skipToPrevious() { controls()?.skipToPrevious() }
    override fun seekTo(positionMs: Long) { controls()?.seekTo(positionMs) }

    companion object {
        const val SPOTIFY_PACKAGE = "com.spotify.music"
        private const val TAG = "MiniLyrics"
        private const val RETRY_MS = 5_000L

        fun hasListenerPermission(context: Context): Boolean =
            NotificationManagerCompat.getEnabledListenerPackages(context).contains(context.packageName)
    }
}
