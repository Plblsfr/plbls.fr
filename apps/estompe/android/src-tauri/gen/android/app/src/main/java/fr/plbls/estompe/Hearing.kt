package fr.plbls.estompe

import org.json.JSONObject
import org.vosk.Model
import org.vosk.Recognizer

const val HEAR_RATE = 16000

private fun resultText(json: String?): String = try { JSONObject(json ?: "{}").optString("text").trim() } catch (e: Exception) { "" }

@Volatile var hearTrace: ((String) -> Unit)? = null

@Volatile var hearDump: ((String, ShortArray, Int) -> Unit)? = null

private fun decode(model: Model, samples: ShortArray, from: Int, count: Int, grammar: String?): String {
  if (count <= 0) return ""
  val rec = if (grammar == null) Recognizer(model, HEAR_RATE.toFloat()) else Recognizer(model, HEAR_RATE.toFloat(), grammar)
  return try {
    rec.acceptWaveForm(if (from == 0) samples else samples.copyOfRange(from, from + count), count)
    resultText(rec.finalResult)
  } finally { rec.close() }
}

fun decodeOrder(model: Model, samples: ShortArray, count: Int, phrases: Set<String>, said: Int = -1): String {
  if (count <= 0) return ""
  val whole = said > 0
  hearDump?.invoke("veille", samples, count)
  var known = ""
  if (phrases.isNotEmpty()) {
    known = decode(model, samples, 0, count, grammarJson(if (whole) phrases.map { "$WAKE_WORD $it" } + WAKE_WORD else phrases))
    val order = if (whole) known.removePrefix(WAKE_WORD).trim() else known
    if (order in phrases || (whole && known == WAKE_WORD)) {
      hearTrace?.invoke("ordre connu « $known »")
      return order
    }
  }
  val free = decode(model, samples, 0, count, null)
  val stripped = if (whole) stripWake(free) else free
  val text = if (whole && stripped == null) decode(model, samples, said, count - said, null) else stripped.orEmpty()
  hearTrace?.invoke("grammaire « $known », dictée « $free », retenu « $text », ${count * 10 / HEAR_RATE / 10.0} s")
  return text
}

class WakeDetector(private val model: Model, private val phrases: () -> Set<String>) {
  private val rec = Recognizer(model, HEAR_RATE.toFloat(), wakeGrammar()).apply { setWords(true) }
  private val ring = ShortArray(HEAR_RATE * KEEP_S)
  private var total = 0L

  fun feed(buf: ShortArray, n: Int): String? {
    for (i in 0 until n) ring[((total + i) % ring.size).toInt()] = buf[i]
    total += n
    if (!rec.acceptWaveForm(buf, n)) return null
    val words = try { JSONObject(rec.result).optJSONArray("result") } catch (e: Exception) { null } ?: return null
    var start = -1.0
    var end = -1.0
    var after = false
    for (i in 0 until words.length()) {
      val w = words.optJSONObject(i) ?: continue
      if (w.optString("word") == WAKE_WORD) { start = w.optDouble("start", -1.0); end = w.optDouble("end", -1.0); after = false } else if (end >= 0) after = true
    }
    if (end < 0 || start < 0) return null
    if (!after) return ""
    val from = maxOf((start * HEAR_RATE).toLong() - LEAD, total - ring.size, 0L)
    val count = (total - from).toInt()
    if (count <= 0) return ""
    val tail = ShortArray(count) { ring[((from + it) % ring.size).toInt()] }
    val said = ((end * HEAR_RATE).toLong() - from).toInt().coerceIn(1, count)
    return decodeOrder(model, tail, count, phrases(), said)
  }

  fun close() {
    rec.close()
  }

  companion object {
    private const val KEEP_S = 15
    private const val LEAD = HEAR_RATE / 5
  }
}

class OrderDecoder(model: Model, private val phrases: Set<String>) {
  private val free = Recognizer(model, HEAR_RATE.toFloat())
  private val known = if (phrases.isEmpty()) null else Recognizer(model, HEAR_RATE.toFloat(), grammarJson(phrases))
  private var said = ""
  private val kept = if (hearDump == null) null else ShortArray(HEAR_RATE * 15)
  private var size = 0

  fun feed(buf: ShortArray, n: Int): String? {
    if (kept != null) { val c = minOf(n, kept.size - size); System.arraycopy(buf, 0, kept, size, c); size += c }
    val k = known
    if (k != null && k.acceptWaveForm(buf, n)) {
      said = (said + " " + resultText(k.result)).trim()
      if (said in phrases) return said
    }
    if (!free.acceptWaveForm(buf, n)) return null
    return pick(resultText(free.result)).also { said = "" }
  }

  fun finish(): String? = pick(resultText(free.finalResult))

  private fun pick(text: String): String? {
    val k = (said + " " + (known?.let { resultText(it.finalResult) } ?: "")).trim()
    if (k.isNotEmpty() || text.isNotEmpty()) hearTrace?.invoke("écoute : grammaire « $k », dictée « $text »")
    return if (k in phrases) k else text.ifEmpty { null }
  }

  fun close() {
    if (kept != null && size > 0) hearDump?.invoke("ecoute", kept, size)
    free.close()
    known?.close()
  }
}
