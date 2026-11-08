package dev.minilyrics.android

import android.content.Context
import android.os.Handler
import android.os.Looper
import dev.minilyrics.android.player.Availability

/** Snapshot status untuk layar MainActivity. */
data class Status(
    val sourceName: String = "Spotify",
    val availability: Availability = Availability.NOT_RUNNING,
    val playerState: String = "-",
    val trackLabel: String? = null,
    val lyricsLabel: String = "-",
    val cardTitle: String = "",
    val cardSubtitle: String = "",
    val autoResumeCount: Int = 0,
    val lastAutoResume: String? = null,
)

/**
 * Status bersama service → activity (satu proses). Semua akses di main thread.
 */
object StatusBus {
    private val main = Handler(Looper.getMainLooper())
    private val listeners = LinkedHashSet<(Status) -> Unit>()

    var current = Status()
        private set

    fun update(transform: (Status) -> Status) {
        val next = transform(current)
        if (next == current) return
        current = next
        main.post { listeners.toList().forEach { it(current) } }
    }

    fun observe(listener: (Status) -> Unit) {
        listeners += listener
        listener(current)
    }

    fun remove(listener: (Status) -> Unit) {
        listeners -= listener
    }
}

/** Setelan kecil app. */
object Prefs {
    const val FILE = "minilyrics"
    const val KEY_DEMO = "demo_mode"

    fun prefs(context: Context) = context.getSharedPreferences(FILE, Context.MODE_PRIVATE)

    /** Mode demo hanya bisa aktif di build debug. */
    fun demoEnabled(context: Context): Boolean =
        BuildConfig.DEBUG && prefs(context).getBoolean(KEY_DEMO, false)
}
