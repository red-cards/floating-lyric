package dev.minilyrics.android.lyrics

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Kasus yang sama dengan test di crates/core/src/lrclib.rs, plus aturan khusus Android. */
class LrclibMatchingTest {
    @Test
    fun cleanTitleCutsSuffixes() {
        assertEquals("Yesterday", LrclibMatching.cleanTitle("Yesterday - Remastered 2009"))
        assertEquals("Song", LrclibMatching.cleanTitle("Song (feat. Someone)"))
        assertEquals("Song", LrclibMatching.cleanTitle("Song (Feat. Someone) - Live"))
        assertEquals("Song", LrclibMatching.cleanTitle("Song [feat. X]"))
        assertEquals("Lagu Biasa", LrclibMatching.cleanTitle("Lagu Biasa"))
        assertEquals("Spasi", LrclibMatching.cleanTitle("  Spasi  "))
    }

    @Test
    fun cleanTitleKeepsHyphenatedWordsAndUnicode() {
        assertEquals("Hip-Hop Song", LrclibMatching.cleanTitle("Hip-Hop Song"))
        assertEquals("夜に駆ける", LrclibMatching.cleanTitle("夜に駆ける - Remix"))
        assertEquals("Ünïcode", LrclibMatching.cleanTitle("Ünïcode (feat. Ä)"))
    }

    @Test
    fun cleanTitleNeverReturnsEmpty() {
        assertEquals("- Intro", LrclibMatching.cleanTitle(" - Intro"))
    }

    @Test
    fun pickBestPrefersClosestDurationWithSynced() {
        val rs = listOf(
            rec(1, 200.0, "[00:01.00]a"),
            rec(2, 181.0, null), // paling dekat tapi tanpa lirik sinkron
            rec(3, 185.0, "[00:01.00]b"),
            rec(4, 170.0, "   "),
        )
        assertEquals(3L, LrclibMatching.pickBest(rs, 180.0)?.id)
    }

    @Test
    fun pickBestWithoutDurationTakesFirstSynced() {
        val rs = listOf(rec(1, 200.0, null), rec(2, null, "[00:01.00]a"))
        assertEquals(2L, LrclibMatching.pickBest(rs, 0.0)?.id)
        assertNull(LrclibMatching.pickBest(listOf(rec(1, 1.0, null)), 1.0))
    }

    @Test
    fun toResultPrefersSyncedThenInstrumentalThenPlain() {
        assertTrue(LrclibMatching.toResult(rec(1, 1.0, "[00:01.00]a")) is LyricsResult.Synced)
        assertEquals(LyricsResult.Instrumental, LrclibMatching.toResult(rec(1, 1.0, null, instrumental = true)))
        assertEquals(LyricsResult.Unsynced, LrclibMatching.toResult(rec(1, 1.0, "bukan lrc", plain = "teks")))
        assertNull(LrclibMatching.toResult(rec(1, 1.0, null)))
    }

    @Test
    fun plainMatchNeedsCloseDuration() {
        assertTrue(LrclibMatching.hasPlainMatch(listOf(rec(1, 183.0, null, plain = "x")), 180.0))
        assertFalse(LrclibMatching.hasPlainMatch(listOf(rec(1, 190.0, null, plain = "x")), 180.0))
        assertFalse(LrclibMatching.hasPlainMatch(listOf(rec(1, 180.0, null, plain = "x")), 0.0))
    }

    private fun rec(id: Long, duration: Double?, synced: String?, plain: String? = null, instrumental: Boolean = false) =
        LrclibRecord(id, null, null, duration, instrumental, plain, synced)
}
