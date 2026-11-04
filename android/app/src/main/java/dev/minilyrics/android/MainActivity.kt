package dev.minilyrics.android

import android.Manifest
import android.app.Activity
import android.content.ComponentName
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.support.v4.media.MediaBrowserCompat
import android.support.v4.media.MediaMetadataCompat
import android.support.v4.media.session.MediaControllerCompat
import android.support.v4.media.session.PlaybackStateCompat
import android.util.Log
import android.view.Gravity
import android.widget.Button
import android.widget.LinearLayout
import android.widget.TextView

/**
 * Layar tes sederhana (tanpa XML layout): konek ke [LyricTestService] lewat
 * MediaBrowserCompat, supaya prototype bisa dicoba di HP tanpa mobil.
 */
class MainActivity : Activity() {

    private lateinit var status: TextView
    private lateinit var playPause: Button

    private var browser: MediaBrowserCompat? = null
    private var controller: MediaControllerCompat? = null

    private val controllerCallback = object : MediaControllerCompat.Callback() {
        override fun onPlaybackStateChanged(state: PlaybackStateCompat?) = render()
        override fun onMetadataChanged(metadata: MediaMetadataCompat?) = render()
        override fun onSessionDestroyed() {
            Log.i(TAG, "activity: session destroyed")
            render()
        }
    }

    private val connectionCallback = object : MediaBrowserCompat.ConnectionCallback() {
        override fun onConnected() {
            val b = browser ?: return
            Log.i(TAG, "activity: browser connected")
            controller = MediaControllerCompat(this@MainActivity, b.sessionToken).also {
                it.registerCallback(controllerCallback)
            }
            render()
        }

        override fun onConnectionSuspended() {
            Log.w(TAG, "activity: browser connection suspended")
            dropController()
        }

        override fun onConnectionFailed() {
            Log.e(TAG, "activity: browser connection failed")
            dropController()
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        Log.i(TAG, "activity onCreate")

        val pad = (16 * resources.displayMetrics.density).toInt()
        status = TextView(this).apply {
            textSize = 18f
            setPadding(0, 0, 0, pad)
        }
        playPause = Button(this).apply { setOnClickListener { togglePlay() } }
        val prev = Button(this).apply {
            text = "‹ Baris sebelumnya"
            setOnClickListener { controller?.transportControls?.skipToPrevious() }
        }
        val next = Button(this).apply {
            text = "Baris berikutnya ›"
            setOnClickListener { controller?.transportControls?.skipToNext() }
        }
        val stop = Button(this).apply {
            text = "Stop"
            setOnClickListener { controller?.transportControls?.stop() }
        }
        setContentView(
            LinearLayout(this).apply {
                orientation = LinearLayout.VERTICAL
                gravity = Gravity.CENTER_HORIZONTAL
                setPadding(pad, pad, pad, pad)
                addView(status)
                addView(playPause)
                addView(prev)
                addView(next)
                addView(stop)
            }
        )
        render()
        requestNotificationPermission()
    }

    override fun onStart() {
        super.onStart()
        browser = MediaBrowserCompat(
            this, ComponentName(this, LyricTestService::class.java), connectionCallback, null
        ).also { it.connect() }
    }

    override fun onStop() {
        dropController()
        browser?.disconnect()
        browser = null
        super.onStop()
    }

    private fun dropController() {
        controller?.unregisterCallback(controllerCallback)
        controller = null
        render()
    }

    private fun togglePlay() {
        val c = controller ?: return
        if (c.playbackState?.state == PlaybackStateCompat.STATE_PLAYING) {
            c.transportControls.pause()
        } else {
            c.transportControls.play()
        }
    }

    private fun render() {
        val c = controller
        if (c == null) {
            status.text = "Belum konek ke service…"
            playPause.text = "Play"
            playPause.isEnabled = false
            return
        }
        val playing = c.playbackState?.state == PlaybackStateCompat.STATE_PLAYING
        val meta = c.metadata
        val title = meta?.getString(MediaMetadataCompat.METADATA_KEY_DISPLAY_TITLE) ?: "–"
        val subtitle = meta?.getString(MediaMetadataCompat.METADATA_KEY_DISPLAY_SUBTITLE) ?: "–"
        status.text = "${if (playing) "▶ Playing" else "❚❚ Paused"}\n\nSekarang: $title\nBerikutnya: $subtitle"
        playPause.text = if (playing) "Pause" else "Play"
        playPause.isEnabled = true
    }

    private fun requestNotificationPermission() {
        if (Build.VERSION.SDK_INT >= 33 &&
            checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) {
            requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), REQ_NOTIF)
        }
    }

    override fun onRequestPermissionsResult(requestCode: Int, permissions: Array<out String>, grantResults: IntArray) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        if (requestCode == REQ_NOTIF) {
            Log.i(TAG, "izin notifikasi: ${grantResults.firstOrNull() == PackageManager.PERMISSION_GRANTED}")
        }
    }

    companion object {
        private const val TAG = LyricTestService.TAG
        private const val REQ_NOTIF = 1
    }
}
