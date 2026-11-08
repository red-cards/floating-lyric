package dev.minilyrics.android.lyrics

import org.junit.Assert.assertEquals
import org.junit.Test

/** Kasus yang sama dengan test di crates/core/src/lrc.rs. */
class LrcTest {
    @Test
    fun parsesBasicDotFormat() {
        val l = Lrc.parse("[00:12.34]Halo dunia\n[01:02.50]Baris dua")
        assertEquals(listOf(LyricLine(12_340, "Halo dunia"), LyricLine(62_500, "Baris dua")), l)
    }

    @Test
    fun parsesColonFractionMillisAndNoFraction() {
        assertEquals(5_200L, Lrc.parse("[00:05:20]Titik dua")[0].timeMs)
        val l = Lrc.parse("[00:01.250]ms\n[00:03]bulat\n[00:04.5]satu digit")
        assertEquals(listOf(1_250L, 3_000L, 4_500L), l.map { it.timeMs })
    }

    @Test
    fun expandsMultipleTimestampsAndSorts() {
        val l = Lrc.parse("[00:30.00]B\n[00:10.00][00:50.00]Reff\n[00:20.00]A")
        assertEquals(
            listOf(10_000L to "Reff", 20_000L to "A", 30_000L to "B", 50_000L to "Reff"),
            l.map { it.timeMs to it.text },
        )
    }

    @Test
    fun keepsEmptyTimedLinesSkipsBlankAndMeta() {
        val input = "[ar:Artis]\n[ti:Judul]\n\n[00:01.00]Satu\n[00:04.00]\n   \nteks tanpa waktu\n[00:06.00] Dua "
        assertEquals(listOf("Satu", "", "Dua"), Lrc.parse(input).map { it.text })
    }

    @Test
    fun handlesCrlf() {
        val l = Lrc.parse("[00:01.00]A\r\n[00:02.00]B\r\n")
        assertEquals(listOf("A", "B"), l.map { it.text })
    }

    @Test
    fun activeIndex() {
        val l = Lrc.parse("[00:01.00]A\n[00:02.00]B\n[00:03.00]C")
        assertEquals(-1, Lrc.activeIndex(l, 999))
        assertEquals(0, Lrc.activeIndex(l, 1_000))
        assertEquals(1, Lrc.activeIndex(l, 2_500))
        assertEquals(2, Lrc.activeIndex(l, 99_000))
        assertEquals(-1, Lrc.activeIndex(emptyList(), 5_000))
    }
}
