package dev.minilyrics.android

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class CardTextTest {
    @Test
    fun shortLineShowsNextLinePreview() {
        assertEquals(
            CardText.Card("Baris pendek", "› Satu baris yang"),
            CardText.forLine("Baris pendek", "Satu baris yang cukup panjang"),
        )
    }

    @Test
    fun longLineSplitsAtWordBoundary() {
        val c = CardText.forLine("Satu baris yang cukup panjang untuk dipecah", "berikutnya")
        assertEquals("Satu baris yang", c.title)
        assertEquals("cukup panjang untuk dipecah", c.subtitle)
        assertTrue(c.title.length <= CardText.CHARS_PER_PART)
    }

    @Test
    fun exactLimitStaysOnTitle() {
        val sixteen = "abcd efgh ijk lm" // 16 karakter
        assertEquals(CardText.Card(sixteen, ""), CardText.forLine(sixteen, null))
    }

    @Test
    fun noSpaceTextIsHardSplitWithoutBreakingEmoji() {
        val (a, b) = CardText.split("日本語のテスト行です、とても長い行")
        assertEquals(16, a.length)
        assertEquals("行", b) // 17 karakter → 16 + 1
        val emoji = "aaaaaaaaaaaaaaa🎵bbb" // emoji (2 char) melewati batas 16
        val (e1, e2) = CardText.split(emoji)
        assertEquals("aaaaaaaaaaaaaaa", e1)
        assertEquals("🎵bbb", e2)
    }

    @Test
    fun emptyLineShowsGapMark() {
        assertEquals(CardText.Card("♪", "› Lanjut"), CardText.forLine("  ", "Lanjut"))
    }

    @Test
    fun lastLineHasEmptySubtitle() {
        assertEquals(CardText.Card("Selesai", ""), CardText.forLine("Selesai", null))
    }

    @Test
    fun collapsesWhitespace() {
        assertEquals(CardText.Card("a b", ""), CardText.forLine("  a \t  b ", null))
    }
}
