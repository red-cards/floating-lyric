package dev.minilyrics.android.player

import android.media.session.PlaybackState
import android.os.Handler
import android.os.SystemClock
import android.util.Log
import dev.minilyrics.android.lyrics.Lrc
import dev.minilyrics.android.lyrics.LyricLine

/**
 * Player palsu untuk tes tanpa Spotify (hanya di build debug, lewat toggle di
 * MainActivity). Lagu dan liriknya buatan sendiri, bukan lirik lagu asli.
 */
class DemoSource(private val handler: Handler) : PlayerSource {
    override val name = "Demo"
    override val availability = Availability.AVAILABLE
    override val embeddedLyrics: List<LyricLine> = Lrc.parse(DEMO_LRC)

    override var track: Track? = null
        private set
    override var playback: Playback? = null
        private set

    private var listener: PlayerSource.Listener? = null
    private var trackNo = 1

    private val loop = Runnable {
        Log.i(TAG, "demo: lagu selesai, ulang dari awal")
        startTrack(trackNo, playing = true)
    }

    override fun start(listener: PlayerSource.Listener) {
        this.listener = listener
        listener.onAvailabilityChanged(availability)
        startTrack(1, playing = false)
    }

    override fun stop() {
        handler.removeCallbacks(loop)
        listener = null
    }

    private fun startTrack(no: Int, playing: Boolean) {
        trackNo = no
        track = Track("Lagu Demo $no", "MiniLyrics", "Album Demo", DURATION_MS, null)
        listener?.onTrackChanged(track)
        setState(if (playing) PlaybackState.STATE_PLAYING else PlaybackState.STATE_PAUSED, 0)
    }

    private fun setState(state: Int, positionMs: Long) {
        val p = Playback(state, positionMs.coerceIn(0, DURATION_MS), SystemClock.elapsedRealtime(), 1f)
        playback = p
        handler.removeCallbacks(loop)
        if (p.isPlaying) handler.postDelayed(loop, DURATION_MS - p.positionMs)
        listener?.onPlaybackChanged(p)
    }

    private fun now() = playback?.positionAt(SystemClock.elapsedRealtime(), DURATION_MS) ?: 0

    override fun play() = setState(PlaybackState.STATE_PLAYING, now())
    override fun pause() = setState(PlaybackState.STATE_PAUSED, now())
    override fun skipToNext() = startTrack(trackNo + 1, playing = playback?.isPlaying == true)
    override fun skipToPrevious() = startTrack(maxOf(1, trackNo - 1), playing = playback?.isPlaying == true)
    override fun seekTo(positionMs: Long) = setState(playback?.state ?: PlaybackState.STATE_PAUSED, positionMs)

    companion object {
        private const val TAG = "MiniLyrics"
        private const val DURATION_MS = 40_000L

        private val DEMO_LRC = """
            [00:03.00]Ini lirik demo MiniLyrics
            [00:07.00]Baris pendek
            [00:10.00]Satu baris yang cukup panjang untuk dipecah jadi dua bagian di card
            [00:15.00]Emoji ikut tampil 🎵🚗✨
            [00:19.00]
            [00:22.00]日本語のテスト行です、とても長い行
            [00:27.00]Hampir selesai
            [00:31.00]Baris terakhir demo, lalu ulang dari awal
        """.trimIndent()
    }
}
