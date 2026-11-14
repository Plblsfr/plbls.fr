package fr.plbls.estompe

import org.json.JSONObject
import org.vosk.Model
import org.vosk.Recognizer

const val HEAR_RATE = 16000

private fun resultText(json: String?): String = try { JSONObject(json ?: "{}").optString("text").trim() } catch (e: Exception) { "" }

fun decodeOrder(model: Model, samples: ShortArray, count: Int, phrases: Set<String>): String {
  if (count <= 0) return ""
  if (phrases.isNotEmpty()) {
    val known = Recognizer(model, HEAR_RATE.toFloat(), grammarJson(phrases))
    val t = try { known.acceptWaveForm(samples, count); resultText(known.finalResult) } finally { known.close() }
    if (t in phrases) return t
  }
  val free = Recognizer(model, HEAR_RATE.toFloat())
  return try { free.acceptWaveForm(samples, count); resultText(free.finalResult) } finally { free.close() }
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
    var end = -1.0
    var after = false
    for (i in 0 until words.length()) {
      val w = words.optJSONObject(i) ?: continue
      if (w.optString("word") == WAKE_WORD) { end = w.optDouble("end", -1.0); after = false } else if (end >= 0) after = true
    }
    if (end < 0) return null
    if (!after) return ""
    val from = maxOf((end * HEAR_RATE).toLong(), total - ring.size, 0L)
    val count = (total - from).toInt()
    if (count <= 0) return ""
    val tail = ShortArray(count) { ring[((from + it) % ring.size).toInt()] }
    return decodeOrder(model, tail, count, phrases())
  }

  fun close() {
    rec.close()
  }

  companion object {
    private const val KEEP_S = 15
  }
}

class OrderDecoder(model: Model, private val phrases: Set<String>) {
  private val free = Recognizer(model, HEAR_RATE.toFloat())
  private val known = if (phrases.isEmpty()) null else Recognizer(model, HEAR_RATE.toFloat(), grammarJson(phrases))
  private var said = ""

  fun feed(buf: ShortArray, n: Int): String? {
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
    return if (k in phrases) k else text.ifEmpty { null }
  }

  fun close() {
    free.close()
    known?.close()
  }
}
