package dev.minilyrics.android.player

import android.service.notification.NotificationListenerService
import android.util.Log

/**
 * Hanya ada supaya app boleh memanggil `MediaSessionManager.getActiveSessions()`.
 * Isi notifikasi tidak dibaca sama sekali.
 */
class SpotifyNotificationListener : NotificationListenerService() {
    override fun onListenerConnected() {
        Log.i("MiniLyrics", "notification listener terhubung")
    }

    override fun onListenerDisconnected() {
        Log.i("MiniLyrics", "notification listener terputus")
    }
}
