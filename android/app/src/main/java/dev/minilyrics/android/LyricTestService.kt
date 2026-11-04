package dev.minilyrics.android

import android.Manifest
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Intent
import android.content.pm.PackageManager
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
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

/**
 * Prototype "proxy media app" untuk Android Auto.
 *
 * Tidak memutar audio sama sekali. MediaSession-nya hanya dipakai untuk
 * menaruh baris lirik (dummy) di metadata: TITLE = baris sekarang,
 * ARTIST = baris berikutnya. Android Auto menampilkan metadata ini di card
 * media dashboard.
 *
 * Sengaja TIDAK meminta audio focus: skenario tes B mau melihat apakah
 * Android Auto sendiri yang mem-pause Spotify saat app ini di-play.
 */
class LyricTestService : MediaBrowserServiceCompat() {

    private lateinit var session: MediaSessionCompat
    private val handler = Handler(Looper.getMainLooper())

    private var index = 0
    private var playing = false
    private var isForeground = false

    private val tick = object : Runnable {
        override fun run() {
            index = (index + 1) % LINES.size
            Log.d(TAG, "tick → baris $index")
            publish()
            handler.postDelayed(this, LINE_MS)
        }
    }

    // ---------------------------------------------------------------- lifecycle

    override fun onCreate() {
        super.onCreate()
        Log.i(TAG, "service onCreate")
        createChannel()

        session = MediaSessionCompat(this, "MiniLyricsSession").apply {
            setCallback(callback)
            setSessionActivity(openAppIntent())
            isActive = true
        }
        sessionToken = session.sessionToken
        publish()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        Log.i(TAG, "onStartCommand action=${intent?.action}")
        // Tombol dari notifikasi MediaStyle datang sebagai MEDIA_BUTTON.
        MediaButtonReceiver.handleIntent(session, intent)
        return START_NOT_STICKY
    }

    override fun onTaskRemoved(rootIntent: Intent?) {
        Log.i(TAG, "onTaskRemoved (playing=$playing)")
        super.onTaskRemoved(rootIntent)
    }

    override fun onDestroy() {
        Log.i(TAG, "service onDestroy")
        handler.removeCallbacks(tick)
        session.isActive = false
        session.release()
        super.onDestroy()
    }

    // ---------------------------------------------------------------- browse tree

    override fun onGetRoot(clientPackageName: String, clientUid: Int, rootHints: Bundle?): BrowserRoot {
        Log.i(TAG, "onGetRoot dari $clientPackageName (uid=$clientUid) hints=${rootHints?.keySet()}")
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
            .setTitle("Mulai tes lirik")
            .setSubtitle("${LINES.size} baris dummy, ganti tiap ${LINE_MS / 1000} detik")
            .build()
        result.sendResult(mutableListOf(MediaItem(desc, MediaItem.FLAG_PLAYABLE)))
    }

    // ---------------------------------------------------------------- transport

    private val callback = object : MediaSessionCompat.Callback() {
        override fun onPlay() {
            Log.i(TAG, "onPlay")
            play()
        }

        override fun onPlayFromMediaId(mediaId: String?, extras: Bundle?) {
            Log.i(TAG, "onPlayFromMediaId mediaId=$mediaId")
            index = 0
            play()
        }

        override fun onPlayFromSearch(query: String?, extras: Bundle?) {
            // Perintah suara ("Hey Google, putar MiniLyrics"); query diabaikan.
            Log.i(TAG, "onPlayFromSearch query=\"$query\"")
            index = 0
            play()
        }

        override fun onPause() {
            Log.i(TAG, "onPause")
            pause()
        }

        override fun onStop() {
            Log.i(TAG, "onStop")
            stop()
        }

        override fun onSkipToNext() {
            Log.i(TAG, "onSkipToNext")
            jumpTo(index + 1)
        }

        override fun onSkipToPrevious() {
            Log.i(TAG, "onSkipToPrevious")
            jumpTo(index - 1)
        }

        override fun onSeekTo(pos: Long) {
            Log.i(TAG, "onSeekTo pos=$pos")
            jumpTo((pos / LINE_MS).toInt())
        }

        override fun onMediaButtonEvent(mediaButtonEvent: Intent?): Boolean {
            Log.i(TAG, "onMediaButtonEvent $mediaButtonEvent")
            return super.onMediaButtonEvent(mediaButtonEvent)
        }
    }

    private fun play() {
        playing = true
        session.isActive = true
        handler.removeCallbacks(tick)
        handler.postDelayed(tick, LINE_MS)
        publish()
        goForeground()
    }

    private fun pause() {
        playing = false
        handler.removeCallbacks(tick)
        publish()
        if (isForeground) {
            // Notifikasi tetap ada (bisa di-resume), tapi service tidak foreground lagi.
            ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_DETACH)
            isForeground = false
        }
        postNotification()
    }

    private fun stop() {
        playing = false
        index = 0
        handler.removeCallbacks(tick)
        publish(PlaybackStateCompat.STATE_STOPPED)
        ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_REMOVE)
        isForeground = false
        NotificationManagerCompat.from(this).cancel(NOTIFICATION_ID)
        stopSelf()
    }

    private fun jumpTo(newIndex: Int) {
        index = Math.floorMod(newIndex, LINES.size)
        if (playing) {
            // Mulai hitung 2 detik lagi dari baris baru.
            handler.removeCallbacks(tick)
            handler.postDelayed(tick, LINE_MS)
        }
        publish()
    }

    // ---------------------------------------------------------------- session state

    private fun publish(stateOverride: Int? = null) {
        val current = LINES[index]
        val next = LINES[(index + 1) % LINES.size]

        session.setMetadata(
            MediaMetadataCompat.Builder()
                .putString(MediaMetadataCompat.METADATA_KEY_MEDIA_ID, MEDIA_ID)
                .putString(MediaMetadataCompat.METADATA_KEY_TITLE, current)
                .putString(MediaMetadataCompat.METADATA_KEY_DISPLAY_TITLE, current)
                .putString(MediaMetadataCompat.METADATA_KEY_ARTIST, next)
                .putString(MediaMetadataCompat.METADATA_KEY_DISPLAY_SUBTITLE, next)
                .putString(MediaMetadataCompat.METADATA_KEY_ALBUM, "MiniLyrics (tes)")
                .putLong(MediaMetadataCompat.METADATA_KEY_DURATION, LINES.size * LINE_MS)
                .build()
        )

        val state = stateOverride
            ?: if (playing) PlaybackStateCompat.STATE_PLAYING else PlaybackStateCompat.STATE_PAUSED
        session.setPlaybackState(
            PlaybackStateCompat.Builder()
                .setActions(ACTIONS)
                .setState(state, index * LINE_MS, if (playing) 1f else 0f)
                .build()
        )
        Log.d(TAG, "publish state=$state index=$index title=\"$current\" next=\"$next\"")

        if (playing || isForeground) postNotification()
    }

    // ---------------------------------------------------------------- foreground & notifikasi

    private fun goForeground() {
        val notification = buildNotification()
        try {
            // Service sudah hidup karena di-bind MediaBrowser; promosikan dulu ke
            // foreground, baru startService supaya tetap hidup saat client unbind.
            ServiceCompat.startForeground(
                this,
                NOTIFICATION_ID,
                notification,
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                    ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK
                } else {
                    0
                },
            )
            isForeground = true
            Log.i(TAG, "startForeground OK")
        } catch (e: Exception) {
            // Android 12+/14 bisa menolak (ForegroundServiceStartNotAllowedException /
            // SecurityException) kalau dipanggil dari background.
            Log.w(TAG, "startForeground gagal: ${e.javaClass.simpleName}: ${e.message}", e)
            postNotification()
            return
        }
        try {
            startService(Intent(this, LyricTestService::class.java))
        } catch (e: Exception) {
            Log.w(TAG, "startService gagal: ${e.javaClass.simpleName}: ${e.message}", e)
        }
    }

    private fun postNotification() {
        if (Build.VERSION.SDK_INT >= 33 &&
            ContextCompat.checkSelfPermission(this, Manifest.permission.POST_NOTIFICATIONS) !=
            PackageManager.PERMISSION_GRANTED
        ) {
            Log.w(TAG, "izin POST_NOTIFICATIONS belum diberikan, notifikasi dilewati")
            return
        }
        NotificationManagerCompat.from(this).notify(NOTIFICATION_ID, buildNotification())
    }

    private fun buildNotification(): android.app.Notification {
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
            .setContentTitle(LINES[index])
            .setContentText(LINES[(index + 1) % LINES.size])
            .setSubText("MiniLyrics (tes)")
            .setContentIntent(openAppIntent())
            .setDeleteIntent(
                MediaButtonReceiver.buildMediaButtonPendingIntent(this, PlaybackStateCompat.ACTION_STOP)
            )
            .setVisibility(NotificationCompat.VISIBILITY_PUBLIC)
            .setOnlyAlertOnce(true)
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
        val channel = NotificationChannel(
            CHANNEL_ID, getString(R.string.channel_name), NotificationManager.IMPORTANCE_LOW
        )
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
        private const val MEDIA_ID = "lyric_test"
        private const val CHANNEL_ID = "lyrics"
        private const val NOTIFICATION_ID = 1
        private const val LINE_MS = 2000L

        private const val ACTIONS =
            PlaybackStateCompat.ACTION_PLAY or
                PlaybackStateCompat.ACTION_PAUSE or
                PlaybackStateCompat.ACTION_PLAY_PAUSE or
                PlaybackStateCompat.ACTION_STOP or
                PlaybackStateCompat.ACTION_SKIP_TO_NEXT or
                PlaybackStateCompat.ACTION_SKIP_TO_PREVIOUS or
                PlaybackStateCompat.ACTION_PLAY_FROM_MEDIA_ID or
                PlaybackStateCompat.ACTION_PLAY_FROM_SEARCH or
                PlaybackStateCompat.ACTION_SEEK_TO

        /** Baris dummy buatan sendiri — bukan lirik lagu asli. */
        val LINES = listOf(
            "1. Baris tes pertama",
            "2. Ini baris kedua",
            "3. Baris ketiga agak lebih panjang dari yang lain",
            "4. Baris keempat 🎵✨🚗",
            "5. Ini baris kelima yang sengaja dibuat sangat panjang sekali supaya kita bisa melihat di titik mana Android Auto memotong teks judul pada card media di dashboard split-screen",
            "6. Baris keenam",
            "7. Hampir selesai, baris ketujuh",
            "8. Baris terakhir, lalu ulang dari awal",
        )
    }
}
