package dev.minilyrics.android

import android.Manifest
import android.app.Activity
import android.content.ActivityNotFoundException
import android.content.ComponentName
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.provider.Settings
import android.support.v4.media.MediaBrowserCompat
import android.support.v4.media.session.MediaControllerCompat
import android.support.v4.media.session.PlaybackStateCompat
import android.util.Log
import android.view.View
import android.widget.Button
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.Switch
import android.widget.TextView
import dev.minilyrics.android.player.Availability
import dev.minilyrics.android.player.SpotifyNotificationListener
import dev.minilyrics.android.player.SpotifySource

/**
 * Onboarding izin + layar status (tanpa XML layout). Kontrol di sini lewat
 * MediaBrowserCompat ke [LyricsService], jadi jalurnya sama dengan Android Auto.
 */
class MainActivity : Activity() {

    private lateinit var permissionBox: LinearLayout
    private lateinit var permissionText: TextView
    private lateinit var statusText: TextView
    private lateinit var cardText: TextView
    private lateinit var resumeText: TextView
    private lateinit var playPause: Button

    private var browser: MediaBrowserCompat? = null
    private var controller: MediaControllerCompat? = null

    private val statusListener: (Status) -> Unit = { render(it) }

    private val connectionCallback = object : MediaBrowserCompat.ConnectionCallback() {
        override fun onConnected() {
            val b = browser ?: return
            controller = MediaControllerCompat(this@MainActivity, b.sessionToken)
            render(StatusBus.current)
        }

        override fun onConnectionSuspended() {
            controller = null
        }

        override fun onConnectionFailed() {
            Log.e(TAG, "activity: koneksi ke service gagal")
            controller = null
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val pad = dp(16)
        val root = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(pad, pad, pad, pad)
        }

        // --- onboarding izin notification listener
        permissionText = text(16f)
        permissionBox = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(0, 0, 0, pad)
            addView(heading("1. Izin akses notifikasi"))
            addView(permissionText)
            addView(button("Buka setelan akses notifikasi") { openListenerSettings() })
            addView(
                text(13f).apply {
                    text = "Android 13+: kalau tombol izinnya abu-abu (\"Restricted setting\"), buka Info aplikasi → " +
                        "menu ⋮ → \"Allow restricted settings\", lalu coba lagi."
                }
            )
            addView(button("Buka Info aplikasi") { openAppDetails() })
        }
        root.addView(permissionBox)

        // --- status
        root.addView(heading("Status"))
        statusText = text(16f)
        root.addView(statusText)
        root.addView(heading("Card Android Auto"))
        cardText = text(18f)
        root.addView(cardText)
        resumeText = text(13f)
        root.addView(resumeText)

        // --- kontrol (diteruskan ke Spotify)
        root.addView(heading("Kontrol (diteruskan ke player)"))
        playPause = button("Play") { togglePlay() }
        root.addView(playPause)
        root.addView(button("‹ Lagu sebelumnya") { controller?.transportControls?.skipToPrevious() })
        root.addView(button("Lagu berikutnya ›") { controller?.transportControls?.skipToNext() })
        root.addView(button("Buka Spotify") { openSpotify() })

        // --- mode demo, hanya di build debug
        if (BuildConfig.DEBUG) {
            root.addView(heading("Debug"))
            @Suppress("DEPRECATION") // android.widget.Switch cukup untuk layar debug
            val demo = Switch(this).apply {
                text = "Mode demo (lagu & lirik palsu, tanpa Spotify)"
                isChecked = Prefs.demoEnabled(this@MainActivity)
                setOnCheckedChangeListener { _, on ->
                    Prefs.prefs(this@MainActivity).edit().putBoolean(Prefs.KEY_DEMO, on).apply()
                }
            }
            root.addView(demo)
        }
        root.addView(text(12f).apply { text = "MiniLyrics ${BuildConfig.VERSION_NAME} · log: adb logcat -s MiniLyrics" })

        setContentView(ScrollView(this).apply { addView(root) })
        requestNotificationPermission()
    }

    override fun onStart() {
        super.onStart()
        StatusBus.observe(statusListener)
        browser = MediaBrowserCompat(this, ComponentName(this, LyricsService::class.java), connectionCallback, null)
            .also { it.connect() }
    }

    override fun onResume() {
        super.onResume()
        render(StatusBus.current) // izin bisa berubah setelah kembali dari Settings
    }

    override fun onStop() {
        StatusBus.remove(statusListener)
        controller = null
        browser?.disconnect()
        browser = null
        super.onStop()
    }

    private fun render(s: Status) {
        val granted = SpotifySource.hasListenerPermission(this)
        val demo = Prefs.demoEnabled(this)
        // Onboarding hanya tampil selama izin belum ada (mode demo tidak butuh izin).
        permissionBox.visibility = if (!granted && !demo) View.VISIBLE else View.GONE
        permissionText.text = if (granted) "✓ Diberikan" else "✗ Belum. MiniLyrics butuh izin ini hanya untuk membaca session media Spotify " +
            "(judul, artis, posisi). Isi notifikasi tidak dibaca."

        val detected = when (s.availability) {
            Availability.AVAILABLE -> "✓ terdeteksi"
            Availability.NOT_RUNNING -> "✗ tidak aktif (putar lagu di ${s.sourceName})"
            Availability.NO_PERMISSION -> "✗ izin belum ada"
        }
        statusText.text = buildString {
            appendLine("${s.sourceName}: $detected")
            appendLine("Status: ${s.playerState}")
            appendLine("Lagu: ${s.trackLabel ?: "-"}")
            append("Lirik: ${s.lyricsLabel}")
        }
        cardText.text = "${s.cardTitle}\n${s.cardSubtitle}"
        resumeText.text = if (s.autoResumeCount == 0) "Auto-resume: belum pernah" else
            "Auto-resume: ${s.autoResumeCount}× · terakhir ${s.lastAutoResume}"
        playPause.text = if (s.playerState == "PLAYING") "Pause" else "Play"
    }

    private fun togglePlay() {
        val c = controller ?: return
        if (c.playbackState?.state == PlaybackStateCompat.STATE_PLAYING) c.transportControls.pause() else c.transportControls.play()
    }

    private fun openListenerSettings() {
        val component = ComponentName(this, SpotifyNotificationListener::class.java)
        val detail = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            Intent(Settings.ACTION_NOTIFICATION_LISTENER_DETAIL_SETTINGS)
                .putExtra(Settings.EXTRA_NOTIFICATION_LISTENER_COMPONENT_NAME, component.flattenToString())
        } else {
            null
        }
        try {
            startActivity(detail ?: Intent(Settings.ACTION_NOTIFICATION_LISTENER_SETTINGS))
        } catch (e: ActivityNotFoundException) {
            startActivity(Intent(Settings.ACTION_NOTIFICATION_LISTENER_SETTINGS))
        }
    }

    private fun openAppDetails() {
        startActivity(Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.fromParts("package", packageName, null)))
    }

    private fun openSpotify() {
        val intent = packageManager.getLaunchIntentForPackage(SpotifySource.SPOTIFY_PACKAGE)
        if (intent != null) startActivity(intent) else Log.w(TAG, "Spotify tidak terpasang")
    }

    private fun requestNotificationPermission() {
        if (Build.VERSION.SDK_INT >= 33 &&
            checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) {
            requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), REQ_NOTIF)
        }
    }

    private fun dp(v: Int) = (v * resources.displayMetrics.density).toInt()
    private fun text(size: Float) = TextView(this).apply { textSize = size }
    private fun heading(t: String) = TextView(this).apply {
        text = t
        textSize = 14f
        setTypeface(typeface, android.graphics.Typeface.BOLD)
        setPadding(0, dp(12), 0, dp(4))
    }
    private fun button(label: String, onClick: () -> Unit) = Button(this).apply {
        text = label
        setOnClickListener { onClick() }
    }

    companion object {
        private const val TAG = LyricsService.TAG
        private const val REQ_NOTIF = 1
    }
}
