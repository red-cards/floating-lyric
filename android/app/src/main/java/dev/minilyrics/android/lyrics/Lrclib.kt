package dev.minilyrics.android.lyrics

import android.util.Log
import org.json.JSONArray
import org.json.JSONObject
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL
import java.net.URLEncoder
import kotlin.math.abs
import kotlin.math.roundToLong

/** Data lagu yang dipakai untuk mencari lirik (setara `TrackQuery` di Rust). */
data class TrackQuery(
    val key: String,
    val title: String,
    val artist: String,
    val album: String,
    val durationSecs: Double,
)

/** Hasil pencarian lirik. */
sealed interface LyricsResult {
    data class Synced(val lines: List<LyricLine>) : LyricsResult
    /** Ada lirik, tapi tanpa timestamp. */
    data object Unsynced : LyricsResult
    data object Instrumental : LyricsResult
    data object NotFound : LyricsResult
    /** Timeout, DNS, HTTP 5xx, dsb. Tidak di-cache; dicoba ulang nanti. */
    data class NetworkError(val message: String) : LyricsResult
}

/** Satu record dari LRCLIB. */
data class LrclibRecord(
    val id: Long?,
    val trackName: String?,
    val artistName: String?,
    val duration: Double?,
    val instrumental: Boolean,
    val plainLyrics: String?,
    val syncedLyrics: String?,
) {
    val synced: String? get() = syncedLyrics?.takeIf { it.isNotBlank() }
    val plain: String? get() = plainLyrics?.takeIf { it.isNotBlank() }
}

/**
 * Logika pencocokan LRCLIB, port dari `crates/core/src/lrclib.rs`
 * (`clean_title`, `pick_best`, alur `fetch`). Fungsi di sini murni
 * (tanpa I/O) supaya bisa dites di JVM.
 */
object LrclibMatching {
    /**
     * Bersihkan judul untuk pencarian: potong setelah `" - "` (mis. "- Remastered
     * 2011") dan setelah `" (feat"` / `" [feat"` / `" (ft."` / `" [ft."`.
     */
    fun cleanTitle(title: String): String {
        var end = title.length
        title.indexOf(" - ").takeIf { it >= 0 }?.let { end = minOf(end, it) }
        for (pat in listOf(" (feat", " [feat", " (ft.", " [ft.")) {
            title.indexOf(pat, ignoreCase = true).takeIf { it >= 0 }?.let { end = minOf(end, it) }
        }
        val cleaned = title.substring(0, end).trim()
        return cleaned.ifEmpty { title.trim() }
    }

    /**
     * Pilih record dengan lirik sinkron yang durasinya paling dekat ke
     * [durationSecs]. Kalau durasi tidak diketahui (<= 0), ambil record
     * pertama yang punya lirik sinkron.
     */
    fun pickBest(records: List<LrclibRecord>, durationSecs: Double): LrclibRecord? {
        val withSynced = records.filter { it.synced != null }
        if (durationSecs <= 0.0) return withSynced.firstOrNull()
        return withSynced.minByOrNull { r -> r.duration?.let { abs(it - durationSecs) } ?: Double.MAX_VALUE }
    }

    /**
     * Tambahan khusus Android (desktop tidak menampilkan lirik tanpa
     * timestamp): record hasil search yang hanya punya lirik biasa dianggap
     * cocok kalau durasinya dekat (±[PLAIN_MAX_DIFF_SECS]).
     */
    fun hasPlainMatch(records: List<LrclibRecord>, durationSecs: Double): Boolean =
        records.any { r ->
            r.plain != null && durationSecs > 0 && r.duration != null &&
                abs(r.duration - durationSecs) <= PLAIN_MAX_DIFF_SECS
        }

    /** Ubah satu record jadi hasil; `null` kalau record tidak berguna. */
    fun toResult(rec: LrclibRecord): LyricsResult? {
        rec.synced?.let { text ->
            val lines = Lrc.parse(text)
            if (lines.isNotEmpty()) return LyricsResult.Synced(lines)
        }
        if (rec.instrumental) return LyricsResult.Instrumental
        if (rec.plain != null) return LyricsResult.Unsynced
        return null
    }

    const val PLAIN_MAX_DIFF_SECS = 5.0
}

/** Klien HTTP LRCLIB (https://lrclib.net), sumber yang sama dengan versi desktop. */
class LrclibClient(
    appVersion: String,
    private val baseUrl: String = BASE_URL,
) {
    private val userAgent = "MiniLyrics-Android/$appVersion (https://github.com/red-cards/floating-lyric)"

    /**
     * Alur sama dengan desktop: `/api/get` (judul, artis, album, durasi) →
     * kalau tidak ada lirik sinkron, `/api/search` (judul dibersihkan + artis)
     * dan pilih durasi terdekat. Blocking: panggil dari Dispatchers.IO.
     */
    fun fetch(q: TrackQuery): LyricsResult {
        if (q.title.isBlank()) return LyricsResult.NotFound
        var exactFallback: LyricsResult? = null
        try {
            // /api/get butuh semua parameter; lewati kalau album/durasi kosong.
            if (q.album.isNotBlank() && q.durationSecs > 0) {
                val rec = getExact(q)
                if (rec != null) {
                    when (val r = LrclibMatching.toResult(rec)) {
                        is LyricsResult.Synced -> return r
                        // Simpan dulu; search mungkin masih menemukan versi sinkron.
                        else -> exactFallback = r
                    }
                }
            }
            val records = search(LrclibMatching.cleanTitle(q.title), q.artist)
            LrclibMatching.pickBest(records, q.durationSecs)
                ?.let { LrclibMatching.toResult(it) }
                ?.let { return it }
            exactFallback?.let { return it }
            if (LrclibMatching.hasPlainMatch(records, q.durationSecs)) return LyricsResult.Unsynced
            return LyricsResult.NotFound
        } catch (e: IOException) {
            Log.w(TAG, "LRCLIB gagal: $e")
            return LyricsResult.NetworkError(e.toString())
        } catch (e: org.json.JSONException) {
            Log.w(TAG, "LRCLIB respons tidak valid: $e")
            return LyricsResult.NetworkError("respons tidak valid: ${e.message}")
        }
    }

    private fun getExact(q: TrackQuery): LrclibRecord? {
        val body = httpGet(
            "/api/get",
            listOf(
                "track_name" to q.title,
                "artist_name" to q.artist,
                "album_name" to q.album,
                "duration" to q.durationSecs.roundToLong().coerceAtLeast(0).toString(),
            ),
        ) ?: return null
        return parseRecord(JSONObject(body))
    }

    private fun search(title: String, artist: String): List<LrclibRecord> {
        val body = httpGet("/api/search", listOf("track_name" to title, "artist_name" to artist))
            ?: return emptyList()
        val arr = JSONArray(body)
        return (0 until arr.length()).map { parseRecord(arr.getJSONObject(it)) }
    }

    /** `null` untuk 404; IOException untuk error jaringan / HTTP non-2xx. */
    private fun httpGet(path: String, params: List<Pair<String, String>>): String? {
        val query = params.joinToString("&") { (k, v) -> "$k=${URLEncoder.encode(v, "UTF-8")}" }
        val url = URL("${baseUrl.trimEnd('/')}$path?$query")
        val started = System.nanoTime()
        val conn = url.openConnection() as HttpURLConnection
        try {
            conn.connectTimeout = 8_000
            conn.readTimeout = 15_000
            conn.setRequestProperty("User-Agent", userAgent)
            conn.setRequestProperty("Accept", "application/json")
            val code = conn.responseCode
            val ms = (System.nanoTime() - started) / 1_000_000
            Log.d(TAG, "GET $path → HTTP $code (${ms} ms)")
            if (code == HttpURLConnection.HTTP_NOT_FOUND) return null
            if (code !in 200..299) throw IOException("GET $path: HTTP $code")
            return conn.inputStream.bufferedReader(Charsets.UTF_8).use { it.readText() }
        } finally {
            conn.disconnect()
        }
    }

    private fun parseRecord(o: JSONObject) = LrclibRecord(
        id = if (o.has("id") && !o.isNull("id")) o.getLong("id") else null,
        trackName = o.optStringOrNull("trackName"),
        artistName = o.optStringOrNull("artistName"),
        duration = if (o.has("duration") && !o.isNull("duration")) o.getDouble("duration") else null,
        instrumental = o.optBoolean("instrumental", false),
        plainLyrics = o.optStringOrNull("plainLyrics"),
        syncedLyrics = o.optStringOrNull("syncedLyrics"),
    )

    private fun JSONObject.optStringOrNull(key: String): String? =
        if (has(key) && !isNull(key)) getString(key) else null

    companion object {
        const val BASE_URL = "https://lrclib.net"
        private const val TAG = "MiniLyrics"
    }
}
