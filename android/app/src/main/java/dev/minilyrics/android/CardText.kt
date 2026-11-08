package dev.minilyrics.android

/**
 * Menyusun teks untuk card media Android Auto.
 *
 * Card hanya punya title + subtitle dan memotong masing-masing sekitar
 * 15–17 karakter (hasil tes di mobil). Jadi satu baris lirik aktif dipecah dua
 * di batas kata: bagian pertama di title, sisanya di subtitle. Kalau barisnya
 * muat di title saja, subtitle diisi potongan awal baris berikutnya dengan
 * prefix "›".
 */
object CardText {
    /** Target jumlah karakter per bagian (title / subtitle). Ubah di sini saja. */
    const val CHARS_PER_PART = 16

    const val NEXT_PREFIX = "› "
    const val GAP_MARK = "♪"

    data class Card(val title: String, val subtitle: String)

    /** Card untuk baris lirik [line] dengan baris berikutnya [next] (boleh null). */
    fun forLine(line: String, next: String?, max: Int = CHARS_PER_PART): Card {
        val text = normalize(line).ifEmpty { GAP_MARK }
        val (first, rest) = split(text, max)
        if (rest.isNotEmpty()) return Card(first, rest)
        val nextText = next?.let(::normalize).orEmpty()
        val preview = if (nextText.isEmpty()) "" else NEXT_PREFIX + head(nextText, max)
        return Card(first, preview)
    }

    /**
     * Pecah [text] jadi dua di batas kata: bagian pertama sepanjang mungkin
     * tapi <= [max] karakter. Kata pertama yang lebih panjang dari [max] (atau
     * teks tanpa spasi, mis. CJK) dipotong paksa di [max] tanpa memecah emoji.
     */
    fun split(text: String, max: Int = CHARS_PER_PART): Pair<String, String> {
        val t = normalize(text)
        if (t.length <= max) return t to ""
        var cut = t.lastIndexOf(' ', max)
        if (cut <= 0) cut = safeCut(t, max)
        return t.substring(0, cut).trimEnd() to t.substring(cut).trim()
    }

    /** Potongan awal [text] (<= [max] karakter, di batas kata kalau bisa). */
    fun head(text: String, max: Int = CHARS_PER_PART): String = split(text, max).first

    private fun normalize(s: String) = s.trim().replace(WHITESPACE, " ")

    /** Indeks potong <= [max] yang tidak membelah surrogate pair. */
    private fun safeCut(t: String, max: Int): Int {
        var cut = max.coerceAtMost(t.length)
        if (cut in 1 until t.length && Character.isHighSurrogate(t[cut - 1]) && Character.isLowSurrogate(t[cut])) cut--
        return cut.coerceAtLeast(1)
    }

    private val WHITESPACE = Regex("\\s+")
}
