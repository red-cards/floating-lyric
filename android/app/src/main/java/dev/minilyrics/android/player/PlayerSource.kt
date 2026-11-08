package dev.minilyrics.android.player

import android.graphics.Bitmap
import android.media.session.PlaybackState
import dev.minilyrics.android.lyrics.LyricLine
import dev.minilyrics.android.lyrics.TrackQuery

/** Lagu yang sedang diputar player sumber (Spotify atau demo). */
data class Track(
    val title: String,
    val artist: String,
    val album: String,
    val durationMs: Long,
    val art: Bitmap?,
) {
    /** Kunci unik lagu; lirik hanya di-fetch ulang saat kunci ini berubah. */
    val key: String get() = "$title|$artist|$album|${durationMs / 1000}"

    fun query() = TrackQuery(key, title, artist, album, durationMs / 1000.0)
}

/**
 * Snapshot PlaybackState player sumber. Posisi diekstrapolasi dari snapshot
 * ini, bukan di-polling: `position + (elapsedRealtime - updateTime) * speed`.
 */
data class Playback(
    /** Konstanta `PlaybackState.STATE_*` (nilainya sama dengan versi compat). */
    val state: Int,
    val positionMs: Long,
    /** `SystemClock.elapsedRealtime()` saat [positionMs] diambil. */
    val updateTimeMs: Long,
    val speed: Float,
) {
    val isPlaying: Boolean get() = state == PlaybackState.STATE_PLAYING

    /** Kecepatan efektif untuk ekstrapolasi (0 kalau tidak sedang playing). */
    val effectiveSpeed: Float get() = if (!isPlaying) 0f else if (speed > 0f) speed else 1f

    fun positionAt(nowElapsedMs: Long, durationMs: Long = 0): Long {
        var pos = positionMs
        if (isPlaying && updateTimeMs > 0) pos += ((nowElapsedMs - updateTimeMs) * effectiveSpeed).toLong()
        if (durationMs > 0) pos = pos.coerceAtMost(durationMs)
        return pos.coerceAtLeast(0)
    }

    companion object {
        fun stateName(state: Int): String = when (state) {
            PlaybackState.STATE_NONE -> "NONE"
            PlaybackState.STATE_STOPPED -> "STOPPED"
            PlaybackState.STATE_PAUSED -> "PAUSED"
            PlaybackState.STATE_PLAYING -> "PLAYING"
            PlaybackState.STATE_FAST_FORWARDING -> "FAST_FORWARDING"
            PlaybackState.STATE_REWINDING -> "REWINDING"
            PlaybackState.STATE_BUFFERING -> "BUFFERING"
            PlaybackState.STATE_ERROR -> "ERROR"
            PlaybackState.STATE_CONNECTING -> "CONNECTING"
            PlaybackState.STATE_SKIPPING_TO_PREVIOUS -> "SKIPPING_TO_PREVIOUS"
            PlaybackState.STATE_SKIPPING_TO_NEXT -> "SKIPPING_TO_NEXT"
            PlaybackState.STATE_SKIPPING_TO_QUEUE_ITEM -> "SKIPPING_TO_QUEUE_ITEM"
            else -> "STATE_$state"
        }
    }
}

/** Kenapa player sumber tidak tersedia (untuk card & layar status). */
enum class Availability { AVAILABLE, NO_PERMISSION, NOT_RUNNING }

/**
 * Player yang lagunya dibaca dan dikendalikan MiniLyrics. Semua callback
 * dipanggil di main thread.
 */
interface PlayerSource {
    interface Listener {
        fun onAvailabilityChanged(availability: Availability)
        fun onTrackChanged(track: Track?)
        fun onPlaybackChanged(playback: Playback?)
    }

    /** Nama untuk log/status, mis. "Spotify" atau "Demo". */
    val name: String
    val availability: Availability
    val track: Track?
    val playback: Playback?

    /** Lirik bawaan sumber (hanya demo); `null` = cari di LRCLIB. */
    val embeddedLyrics: List<LyricLine>? get() = null

    fun start(listener: Listener)
    fun stop()

    fun play()
    fun pause()
    fun skipToNext()
    fun skipToPrevious()
    fun seekTo(positionMs: Long)
}
