package dev.minilyrics.android.lyrics

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Test

/**
 * Tes ke server LRCLIB asli. Hanya jalan kalau env LRCLIB_LIVE=1:
 * `LRCLIB_LIVE=1 ./gradlew testDebugUnitTest --tests '*LrclibLiveTest*'`
 */
class LrclibLiveTest {
    private val client = LrclibClient("test")

    @Before
    fun onlyWhenEnabled() = assumeTrue(System.getenv("LRCLIB_LIVE") == "1")

    @Test
    fun exactGetFindsSyncedLyrics() {
        val r = client.fetch(TrackQuery("k", "Bohemian Rhapsody", "Queen", "A Night at the Opera", 355.0))
        assertTrue("dapat: $r", r is LyricsResult.Synced && r.lines.size > 20)
        println("get: ${(r as LyricsResult.Synced).lines.size} baris, pertama=${r.lines.first()}")
    }

    @Test
    fun searchFallbackWithRemasteredTitleAndUnknownAlbum() {
        val r = client.fetch(TrackQuery("k", "Yesterday - Remastered 2009", "The Beatles", "", 125.0))
        assertTrue("dapat: $r", r is LyricsResult.Synced)
        println("search: ${(r as LyricsResult.Synced).lines.take(2)}")
    }

    @Test
    fun nonsenseIsNotFound() {
        val r = client.fetch(TrackQuery("k", "zzqx tidak ada lagu ini 98123", "Bukan Artis Nyata 77", "", 200.0))
        assertEquals(LyricsResult.NotFound, r)
    }
}
