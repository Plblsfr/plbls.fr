package fr.plbls.estompe

import android.content.Context
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.ParcelFileDescriptor
import android.os.SystemClock
import java.io.File
import java.io.FileInputStream
import java.net.HttpURLConnection
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors
import java.util.concurrent.RejectedExecutionException
import java.util.concurrent.atomic.AtomicBoolean
import kotlin.concurrent.thread
import org.nehuatl.llamacpp.LlamaContext

object LlmModel {
  const val AVAILABLE = true
  const val URL = "https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF/resolve/main/qwen2.5-0.5b-instruct-q4_k_m.gguf"
  const val MIN_BYTES = 50L * 1024 * 1024
  const val MAX_BYTES = 700L * 1024 * 1024
  private const val NAME = "qwen2.5-0.5b-instruct-q4_k_m.gguf"
  private const val MARGIN = 64L * 1024 * 1024

  @Volatile var state = ""

  private fun dir(c: Context) = File(c.filesDir, "llm")
  private fun file(c: Context) = File(dir(c), NAME)
  fun supported(): Boolean = try { Build.SUPPORTED_ABIS.contains("arm64-v8a") } catch (e: Throwable) { false }
  fun ready(c: Context): Boolean = file(c).let { it.isFile && it.length() in MIN_BYTES..MAX_BYTES }
  fun path(c: Context): String = file(c).path

  private class Refused(message: String) : Exception(message)

  fun download(c: Context) {
    if (!AVAILABLE || state == "busy" || ready(c)) return
    state = "busy"
    val ctx = c.applicationContext
    thread(name = "estompe-llm-download") {
      val d = dir(ctx)
      val tmp = File(d, "$NAME.part")
      val start = SystemClock.elapsedRealtime()
      try {
        if (!d.isDirectory && !d.mkdirs()) throw Refused("stockage indisponible")
        tmp.delete()
        val conn = java.net.URL(URL).openConnection() as HttpURLConnection
        try {
          conn.connectTimeout = 15000; conn.readTimeout = 30000; conn.instanceFollowRedirects = true
          conn.setRequestProperty("Accept-Encoding", "identity")
          val code = conn.responseCode
          if (code != 200) throw Refused("réponse $code")
          if (conn.url.protocol != "https") throw Refused("redirection refusée")
          val announced = conn.getHeaderField("Content-Length")?.toLongOrNull() ?: -1L
          if (announced > MAX_BYTES || announced in 0 until MIN_BYTES) throw Refused("taille inattendue")
          val needed = (if (announced > 0) announced else MAX_BYTES) + MARGIN
          if (needed > d.usableSpace) throw Refused("espace insuffisant")
          var total = 0L
          conn.inputStream.use { input ->
            tmp.outputStream().use { out ->
              val buf = ByteArray(64 * 1024)
              while (true) {
                val n = input.read(buf)
                if (n < 0) break
                total += n
                if (total > MAX_BYTES) throw Refused("taille inattendue")
                out.write(buf, 0, n)
              }
              out.fd.sync()
            }
          }
          if (total < MIN_BYTES || (announced >= 0 && total != announced)) throw Refused("téléchargement incomplet")
        } finally { conn.disconnect() }
        if (!isGguf(tmp)) throw Refused("fichier invalide")
        if (!tmp.renameTo(file(ctx))) throw Refused("enregistrement impossible")
        DriveService.note("llm : modèle téléchargé en ${(SystemClock.elapsedRealtime() - start) / 1000} s")
        state = ""
      } catch (e: Refused) {
        DriveService.note("llm : téléchargement refusé (${e.message})")
        state = "téléchargement impossible : ${e.message}"
      } catch (e: Exception) {
        DriveService.note("llm : téléchargement impossible (${e.javaClass.simpleName})")
        state = "téléchargement impossible : réseau ou stockage"
      } finally {
        tmp.delete()
      }
    }
  }

  fun delete(c: Context) {
    if (state == "busy") return
    file(c).delete()
    File(dir(c), "$NAME.part").delete()
    state = ""
  }
}

internal fun isGguf(f: File): Boolean = try {
  FileInputStream(f).use { input ->
    val b = ByteArray(8)
    var n = 0
    while (n < 8) { val r = input.read(b, n, 8 - n); if (r < 0) break; n += r }
    val version = (b[4].toInt() and 0xff) or ((b[5].toInt() and 0xff) shl 8) or ((b[6].toInt() and 0xff) shl 16) or ((b[7].toInt() and 0xff) shl 24)
    n == 8 && b[0] == 'G'.code.toByte() && b[1] == 'G'.code.toByte() && b[2] == 'U'.code.toByte() && b[3] == 'F'.code.toByte() && version in 2..3
  }
} catch (e: Exception) { false }

private val INTENTS = listOf("read_unread", "read_from", "reply", "send_sms", "music_play", "music_pause", "music_next", "music_previous",
  "volume_up", "volume_down", "play_search", "play_playlist", "cancel", "help", "unknown")

private const val MAX_HEARD = 300

private val LLM_SYSTEM = """Tu es le module de compréhension de l'assistant vocal d'Estompe. Tu convertis la phrase entendue en un seul objet JSON sur une ligne, sans aucun autre texte.
Le champ intent prend exactement une de ces valeurs :
- read_unread : lire les messages non lus
- read_from : lire les messages d'un contact, champ contact obligatoire
- reply : répondre au dernier message lu
- send_sms : envoyer un SMS, champ contact obligatoire, champ text avec le message dicté ou vide s'il n'est pas dicté
- music_play, music_pause, music_next, music_previous : reprendre la lecture, pause, morceau suivant, morceau précédent
- volume_up, volume_down : monter ou baisser le volume
- play_search : jouer un artiste, un titre ou un album, champ query obligatoire
- play_playlist : lancer une playlist, champ name obligatoire
- cancel : annuler
- help : demander ce que l'assistant sait faire
- unknown : tout le reste, y compris les questions générales et les phrases qui ne s'adressent pas à l'assistant
Ne devine jamais un contact, une recherche ou un message absent de la phrase : dans le doute, réponds unknown."""

internal fun jsonString(s: String): String = buildString {
  append('"')
  for (c in s) when {
    c == '"' -> append("\\\"")
    c == '\\' -> append("\\\\")
    c == '\n' -> append("\\n")
    c < ' ' -> append("\\u").append(String.format("%04x", c.code))
    else -> append(c)
  }
  append('"')
}

private fun cleanForPrompt(s: String): String = s.replace("<|", " ").replace("|>", " ").map { if (it < ' ') ' ' else it }.joinToString("").trim()

fun llmPrompt(heard: String, apps: List<MusicApp>): String = buildString {
  append("<|im_start|>system\n").append(LLM_SYSTEM).append('\n')
  if (apps.isEmpty()) append("Aucune app de musique n'est proposée : n'écris jamais le champ app.")
  else append("Champ app facultatif pour la musique, seulement si la phrase nomme une app, avec l'identifiant exact : ")
    .append(apps.joinToString(", ") { "${jsonString(cleanForPrompt(it.id))} pour ${cleanForPrompt(it.label)}" }).append('.')
  append("<|im_end|>\n")
  val examples = mutableListOf(
    "mets un peu de daft punk" to "{\"intent\":\"play_search\",\"query\":\"daft punk\"}",
    "écris à marie que j'arrive dans dix minutes" to "{\"intent\":\"send_sms\",\"contact\":\"marie\",\"text\":\"j'arrive dans dix minutes\"}",
    "qu'est-ce que paul m'a écrit" to "{\"intent\":\"read_from\",\"contact\":\"paul\"}",
    "coupe la musique" to "{\"intent\":\"music_pause\"}",
    "plus fort" to "{\"intent\":\"volume_up\"}",
    "quel temps fera-t-il demain" to "{\"intent\":\"unknown\"}",
  )
  apps.firstOrNull()?.let { a ->
    examples += "lance ma playlist route sur ${cleanForPrompt(a.label)}" to "{\"intent\":\"play_playlist\",\"name\":\"route\",\"app\":${jsonString(cleanForPrompt(a.id))}}"
  }
  for ((q, a) in examples) append("<|im_start|>user\n").append(q).append("<|im_end|>\n<|im_start|>assistant\n").append(a).append("<|im_end|>\n")
  append("<|im_start|>user\n").append(cleanForPrompt(heard).take(MAX_HEARD)).append("<|im_end|>\n<|im_start|>assistant\n")
}

private fun gbnf(s: String): String = buildString {
  append('"')
  for (c in s) when (c) { '"' -> append("\\\""); '\\' -> append("\\\\"); else -> append(c) }
  append('"')
}

fun llmGrammar(apps: List<MusicApp>): String {
  val ids = apps.map { cleanForPrompt(it.id) }.filter { it.isNotEmpty() }.distinct()
  val app = if (ids.isEmpty()) "" else " app"
  return buildString {
    append("root ::= \"{\\\"intent\\\":\" body \"}\"\n")
    append("body ::= \"\\\"\" simple \"\\\"\" | \"\\\"read_from\\\",\\\"contact\\\":\" str | \"\\\"send_sms\\\",\\\"contact\\\":\" str \",\\\"text\\\":\" str")
    append(" | \"\\\"\" transport \"\\\"\"$app | \"\\\"play_search\\\",\\\"query\\\":\" str$app | \"\\\"play_playlist\\\",\\\"name\\\":\" str$app\n")
    append("simple ::= \"read_unread\" | \"reply\" | \"cancel\" | \"help\" | \"unknown\"\n")
    append("transport ::= \"music_play\" | \"music_pause\" | \"music_next\" | \"music_previous\" | \"volume_up\" | \"volume_down\"\n")
    if (ids.isNotEmpty()) {
      append("app ::= (\",\\\"app\\\":\" appid)?\n")
      append("appid ::= ").append(ids.joinToString(" | ") { gbnf(jsonString(it)) }).append('\n')
    }
    append("str ::= \"\\\"\" char* \"\\\"\"\n")
    append("char ::= [^\"\\\\\\x00-\\x1f] | \"\\\\\" ([\"\\\\/bfnrt] | \"u\" [0-9a-fA-F] [0-9a-fA-F] [0-9a-fA-F] [0-9a-fA-F])\n")
  }
}

private class FlatJson(private val s: String, private var i: Int) {
  fun obj(): Map<String, String>? {
    val out = LinkedHashMap<String, String>()
    if (!eat('{')) return null
    ws()
    if (eat('}')) return out
    while (true) {
      ws()
      val k = str() ?: return null
      ws()
      if (!eat(':')) return null
      ws()
      val v = str() ?: return null
      if (out.put(k, v) != null || out.size > MAX_FIELDS) return null
      ws()
      if (eat('}')) return out
      if (!eat(',')) return null
    }
  }

  private fun ws() { while (i < s.length && (s[i] == ' ' || s[i] == '\t' || s[i] == '\n' || s[i] == '\r')) i++ }

  private fun eat(c: Char): Boolean = if (i < s.length && s[i] == c) { i++; true } else false

  private fun str(): String? {
    if (!eat('"')) return null
    val b = StringBuilder()
    while (i < s.length) {
      val c = s[i++]
      when {
        c == '"' -> return b.toString()
        c == '\\' -> {
          if (i >= s.length) return null
          when (val e = s[i++]) {
            '"', '\\', '/' -> b.append(e)
            'b' -> b.append('\b')
            'f' -> b.append('\u000C')
            'n' -> b.append('\n')
            'r' -> b.append('\r')
            't' -> b.append('\t')
            'u' -> {
              if (i + 4 > s.length) return null
              val hex = s.substring(i, i + 4)
              if (!hex.all { it in '0'..'9' || it in 'a'..'f' || it in 'A'..'F' }) return null
              b.append(hex.toInt(16).toChar()); i += 4
            }
            else -> return null
          }
        }
        c < ' ' -> return null
        else -> b.append(c)
      }
      if (b.length > MAX_STRING) return null
    }
    return null
  }

  companion object {
    const val MAX_FIELDS = 8
    const val MAX_STRING = 1000
  }
}

private const val MAX_OUTPUT = 4000

fun commandFromLlm(output: String, apps: List<MusicApp>): Command {
  if (output.length > MAX_OUTPUT) return Command.Unknown
  val start = output.indexOf('{')
  if (start < 0) return Command.Unknown
  val fields = FlatJson(output, start).obj() ?: return Command.Unknown
  val intent = fields["intent"] ?: return Command.Unknown
  if (intent !in INTENTS) return Command.Unknown
  fun optional(name: String, max: Int, newlines: Boolean = false): String? {
    val v = (fields[name] ?: return "").trim()
    if (v.length > max || v.any { it.isISOControl() && !(newlines && it == '\n') }) return null
    return v
  }
  fun required(name: String, max: Int): String? = optional(name, max)?.takeIf { it.isNotEmpty() }
  val app = fields["app"]?.trim()?.let { a -> apps.firstOrNull { it.id == a && a.isNotEmpty() }?.id } ?: ""
  return when (intent) {
    "read_unread" -> Command.ReadUnread
    "read_from" -> Command.ReadFrom(required("contact", 80) ?: return Command.Unknown)
    "reply" -> Command.Reply
    "send_sms" -> Command.SendSms(required("contact", 80) ?: return Command.Unknown, optional("text", 500, true) ?: return Command.Unknown)
    "music_play" -> Command.Music(Transport.PLAY, app)
    "music_pause" -> Command.Music(Transport.PAUSE, app)
    "music_next" -> Command.Music(Transport.NEXT, app)
    "music_previous" -> Command.Music(Transport.PREVIOUS, app)
    "volume_up" -> Command.Music(Transport.VOLUME_UP, app)
    "volume_down" -> Command.Music(Transport.VOLUME_DOWN, app)
    "play_search" -> Command.PlaySearch(required("query", 120) ?: return Command.Unknown, app)
    "play_playlist" -> Command.PlayPlaylist(required("name", 120) ?: return Command.Unknown, app)
    "cancel" -> Command.Cancel
    "help" -> Command.Help
    else -> Command.Unknown
  }
}

class Llm(context: Context) {
  private val app = context.applicationContext
  private val ui = Handler(Looper.getMainLooper())
  private val lock = Any()
  private var worker: ExecutorService? = null
  @Volatile private var model: LlamaContext? = null
  private var generating = false
  private val threads = Runtime.getRuntime().availableProcessors().coerceIn(1, 4)

  fun understand(heard: String, apps: List<MusicApp>, done: (Command) -> Unit) {
    val answered = AtomicBoolean(false)
    val answer = { c: Command -> if (answered.compareAndSet(false, true)) ui.post { done(c) } }
    if (!LlmModel.AVAILABLE || heard.isBlank() || !LlmModel.supported() || !LlmModel.ready(app)) { answer(Command.Unknown); return }
    val start = SystemClock.elapsedRealtime()
    val guard = Runnable {
      if (answered.get()) return@Runnable
      stop()
      DriveService.note("llm : délai dépassé (${SystemClock.elapsedRealtime() - start} ms)")
      answer(Command.Unknown)
    }
    ui.postDelayed(guard, TIMEOUT_MS)
    try {
      executor().execute {
        val c = try { if (answered.get()) Command.Unknown else run(heard, apps, answered) } catch (t: Throwable) {
          DriveService.note("llm : échec (${t.javaClass.simpleName})")
          Command.Unknown
        }
        ui.removeCallbacks(guard)
        if (!answered.get()) DriveService.note("llm : réponse en ${SystemClock.elapsedRealtime() - start} ms")
        answer(c)
      }
    } catch (e: RejectedExecutionException) {
      ui.removeCallbacks(guard)
      answer(Command.Unknown)
    }
  }

  private fun run(heard: String, apps: List<MusicApp>, answered: AtomicBoolean): Command {
    val m = load()
    if (answered.get()) return Command.Unknown
    val params = hashMapOf<String, Any>("prompt" to llmPrompt(heard, apps), "grammar" to llmGrammar(apps), "temperature" to 0.0, "top_k" to 1,
      "n_predict" to MAX_TOKENS, "n_threads" to threads, "seed" to 0, "stop" to listOf("<|im_end|>", "<|endoftext|>"), "emit_partial_completion" to false)
    synchronized(lock) { generating = true }
    val out = try { m.completion(params) } finally { synchronized(lock) { generating = false } }
    if (answered.get()) return Command.Unknown
    return commandFromLlm(out["text"] as? String ?: "", apps)
  }

  private fun load(): LlamaContext {
    model?.let { return it }
    val f = File(LlmModel.path(app))
    if (!LlmModel.ready(app) || !isGguf(f)) throw IllegalStateException("modèle absent")
    val start = SystemClock.elapsedRealtime()
    val m = ParcelFileDescriptor.open(f, ParcelFileDescriptor.MODE_READ_ONLY).use { pfd ->
      LlamaContext(1, mapOf<String, Any>("model" to f.path, "model_fd" to pfd.fd, "n_ctx" to CONTEXT, "n_batch" to 512, "n_threads" to threads,
        "use_mlock" to false, "use_mmap" to true, "embedding" to false))
    }
    if (m.context == 0L) throw IllegalStateException("chargement impossible")
    synchronized(lock) { model = m }
    DriveService.note("llm : modèle chargé en ${SystemClock.elapsedRealtime() - start} ms")
    return m
  }

  private fun executor(): ExecutorService = synchronized(lock) {
    worker?.takeIf { !it.isShutdown } ?: Executors.newSingleThreadExecutor { r -> Thread(r, "estompe-llm").apply { isDaemon = true } }.also { worker = it }
  }

  private fun stop() {
    synchronized(lock) { if (generating) try { model?.stopCompletion() } catch (t: Throwable) { DriveService.note("llm : arrêt impossible (${t.javaClass.simpleName})") } }
  }

  fun close() {
    stop()
    val w = synchronized(lock) { worker.also { worker = null } }
    val free = Runnable {
      val m = synchronized(lock) { model.also { model = null } }
      try { m?.release() } catch (t: Throwable) { DriveService.note("llm : libération impossible (${t.javaClass.simpleName})") }
    }
    if (w == null) { if (model != null) thread(name = "estompe-llm-release") { free.run() }; return }
    try { w.execute(free) } catch (e: RejectedExecutionException) { DriveService.note("llm : libération refusée") }
    w.shutdown()
  }

  private companion object {
    const val TIMEOUT_MS = 8000L
    const val MAX_TOKENS = 160
    const val CONTEXT = 2048
  }
}
