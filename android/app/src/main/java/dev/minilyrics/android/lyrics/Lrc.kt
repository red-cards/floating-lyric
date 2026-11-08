package dev.minilyrics.android.lyrics

/** Satu baris lirik tersinkron. */
data class LyricLine(val timeMs: Long, val text: String)

/**
 * Parser LRC. Port dari `crates/core/src/lrc.rs`, aturannya sama:
 *
 * - Satu baris boleh punya beberapa timestamp (`[00:10.00][01:20.00]Reff`).
 * - Mendukung `[mm:ss]`, `[mm:ss.xx]`, `[mm:ss:xx]`, `[mm:ss.xxx]`.
 * - Tag metadata (`[ar:...]`) dan baris tanpa timestamp diabaikan.
 * - Baris bertimestamp dengan teks kosong dipertahankan (penanda jeda).
 * - Hasil diurutkan stabil berdasarkan waktu.
 */
object Lrc {
    private val TIMESTAMP = Regex("""\[(\d{1,3}):(\d{1,2})(?:[.:](\d{1,3}))?]""")

    fun parse(input: String): List<LyricLine> {
        val out = ArrayList<LyricLine>()
        for (rawLine in input.lineSequence()) {
            var rest = rawLine.trim()
            val times = ArrayList<Long>()
            // Timestamp hanya dihitung kalau berada di awal baris (berurutan).
            while (true) {
                val m = TIMESTAMP.find(rest) ?: break
                if (m.range.first != 0) break
                val min = m.groupValues[1].toLong()
                val sec = m.groupValues[2].toLong()
                val fracDigits = m.groupValues[3]
                val fracMs = if (fracDigits.isEmpty()) {
                    0L
                } else {
                    // "5" = 500 ms, "25" = 250 ms, "250" = 250 ms
                    Math.round(fracDigits.toLong() * 1000.0 / Math.pow(10.0, fracDigits.length.toDouble()))
                }
                times += min * 60_000 + sec * 1000 + fracMs
                rest = rest.substring(m.range.last + 1).trimStart()
            }
            if (times.isEmpty()) continue
            val text = rest.trim()
            times.forEach { out += LyricLine(it, text) }
        }
        // sortedBy stabil: baris dengan waktu sama tetap urut sesuai file.
        return out.sortedBy { it.timeMs }
    }

    /** Indeks baris aktif: baris terakhir dengan waktu <= [positionMs], atau -1. */
    fun activeIndex(lines: List<LyricLine>, positionMs: Long): Int {
        var lo = 0
        var hi = lines.size
        while (lo < hi) {
            val mid = (lo + hi) ushr 1
            if (lines[mid].timeMs <= positionMs) lo = mid + 1 else hi = mid
        }
        return lo - 1
    }
}
