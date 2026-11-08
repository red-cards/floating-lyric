package dev.minilyrics.android.lyrics

import android.content.Context
import android.util.Log
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.security.MessageDigest

/** Hasil lookup + dari mana asalnya (untuk layar status & log). */
data class LyricsLookup(val result: LyricsResult, val source: String)

/**
 * Cache lirik per lagu: memori (LRU kecil) + disk sederhana (satu file JSON
 * per lagu di `filesDir/lyrics`). Yang disimpan ke disk hanya hasil yang
 * stabil (sinkron, tidak sinkron, instrumental). "Tidak ketemu" hanya di
 * memori supaya lirik yang baru ditambahkan ke LRCLIB tetap bisa ditemukan
 * setelah app restart. Error jaringan tidak di-cache sama sekali.
 */
class LyricsRepository(context: Context, appVersion: String) {
    private val client = LrclibClient(appVersion)
    private val dir = File(context.filesDir, "lyrics")
    private val memory = object : LinkedHashMap<String, LyricsResult>(32, 0.75f, true) {
        override fun removeEldestEntry(eldest: MutableMap.MutableEntry<String, LyricsResult>) = size > MEMORY_ENTRIES
    }

    suspend fun lookup(q: TrackQuery): LyricsLookup = withContext(Dispatchers.IO) {
        synchronized(memory) { memory[q.key] }?.let { return@withContext LyricsLookup(it, "cache memori") }
        readDisk(q.key)?.let { cached ->
            synchronized(memory) { memory[q.key] = cached }
            return@withContext LyricsLookup(cached, "cache disk")
        }
        val started = System.currentTimeMillis()
        val result = client.fetch(q)
        Log.i(TAG, "LRCLIB \"${q.title}\" — ${q.artist}: ${describe(result)} (${System.currentTimeMillis() - started} ms)")
        if (result !is LyricsResult.NetworkError) synchronized(memory) { memory[q.key] = result }
        if (result is LyricsResult.Synced || result is LyricsResult.Unsynced || result is LyricsResult.Instrumental) {
            writeDisk(q.key, result)
        }
        LyricsLookup(result, "LRCLIB")
    }

    private fun fileFor(key: String): File {
        val hash = MessageDigest.getInstance("SHA-1").digest(key.toByteArray())
            .joinToString("") { "%02x".format(it) }
        return File(dir, "$hash.json")
    }

    private fun readDisk(key: String): LyricsResult? = try {
        val f = fileFor(key)
        if (!f.isFile) {
            null
        } else {
            val o = JSONObject(f.readText())
            when {
                o.optInt("v") != DISK_VERSION || o.optString("key") != key -> null
                else -> when (o.getString("type")) {
                    "synced" -> {
                        val arr = o.getJSONArray("lines")
                        LyricsResult.Synced(
                            (0 until arr.length()).map {
                                val l = arr.getJSONArray(it)
                                LyricLine(l.getLong(0), l.getString(1))
                            }
                        )
                    }
                    "unsynced" -> LyricsResult.Unsynced
                    "instrumental" -> LyricsResult.Instrumental
                    else -> null
                }
            }
        }
    } catch (e: Exception) {
        Log.w(TAG, "cache disk rusak, diabaikan: $e")
        null
    }

    private fun writeDisk(key: String, result: LyricsResult) {
        try {
            dir.mkdirs()
            val o = JSONObject().put("v", DISK_VERSION).put("key", key)
            when (result) {
                is LyricsResult.Synced -> {
                    val arr = JSONArray()
                    result.lines.forEach { arr.put(JSONArray().put(it.timeMs).put(it.text)) }
                    o.put("type", "synced").put("lines", arr)
                }
                LyricsResult.Unsynced -> o.put("type", "unsynced")
                LyricsResult.Instrumental -> o.put("type", "instrumental")
                else -> return
            }
            fileFor(key).writeText(o.toString())
        } catch (e: Exception) {
            Log.w(TAG, "gagal menulis cache disk: $e")
        }
    }

    companion object {
        private const val TAG = "MiniLyrics"
        private const val MEMORY_ENTRIES = 50
        private const val DISK_VERSION = 1

        fun describe(r: LyricsResult): String = when (r) {
            is LyricsResult.Synced -> "sinkron (${r.lines.size} baris)"
            LyricsResult.Unsynced -> "tidak sinkron"
            LyricsResult.Instrumental -> "instrumental"
            LyricsResult.NotFound -> "tidak ketemu"
            is LyricsResult.NetworkError -> "error jaringan: ${r.message}"
        }
    }
}
