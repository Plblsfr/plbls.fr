package fr.plbls.estompe

import java.text.Normalizer

/** Message reçu par notification (champs FCM « data »), dans la forme utile à la voix et à MessagingStyle. */
data class Incoming(
  val tag: String,
  val kind: String,
  val conv: String,
  val msg: String,
  val sender: String,
  val room: String,
  val text: String,
  val attach: String,
  val title: String,
) {
  /** Aperçu masqué par l'utilisateur : le serveur n'envoie ni auteur ni contenu, seulement « Nouveau message de X ». */
  val hidden get() = sender.isEmpty()

  /** Réponse possible : message de conversation (pas d'activité vocale). */
  val replyable get() = conv.isNotEmpty() && msg.isNotEmpty()

  companion object {
    /** Seuls les messages privés et les mentions sont lus en mode conduite. */
    val SPOKEN_KINDS = setOf("dm", "mention")

    fun from(d: Map<String, String>): Incoming? {
      val tag = d["tag"] ?: return null
      return Incoming(tag, d["kind"].orEmpty(), d["conv"].orEmpty(), d["msg"].orEmpty(), d["sender"].orEmpty(),
        d["room"].orEmpty(), d["text"].orEmpty(), d["attach"].orEmpty(), d["title"].orEmpty())
    }
  }
}

/** Texte lu à voix haute : liens, markdown et smileys résumés ou retirés. */
fun spoken(text: String): String {
  val noLinks = text.replace(Regex("https?://\\S+"), "un lien")
    .replace(Regex(":[a-z0-9_+-]+:"), "") // smileys personnalisés
    .replace(Regex("[*_`~>#|]"), "")
  val sb = StringBuilder()
  noLinks.codePoints().forEach { cp ->
    val emoji = cp >= 0x1F000 || cp == 0x200D || cp == 0xFE0F || Character.getType(cp) == Character.OTHER_SYMBOL.toInt()
    if (!emoji) sb.appendCodePoint(cp)
  }
  return sb.toString().replace(Regex("\\s+"), " ").trim()
}

/** « Message de X dans Y : contenu » ; sans le contenu si l'utilisateur ne veut que l'expéditeur. */
fun announce(m: Incoming, readContent: Boolean): String {
  if (m.hidden) return m.title.ifEmpty { "Nouveau message" }
  val head = "Message de ${m.sender}" + if (m.room.isNotEmpty()) " dans ${m.room}" else ""
  if (!readContent) return head
  val attach = when (m.attach) { "image" -> "a envoyé une image"; "" -> null; else -> "a envoyé un fichier" }
  val body = spoken(m.text).ifEmpty { if (m.text.isNotBlank() && attach == null) "a envoyé un smiley" else null }
  val content = listOfNotNull(body, attach).joinToString(". ")
  return if (content.isEmpty()) head else "$head : $content"
}

/** File de lecture : les messages arrivés pendant une lecture sont lus à la suite, sans doublon. */
class ReadQueue {
  private val items = ArrayDeque<Incoming>()
  private val seen = HashSet<String>()

  @Synchronized fun offer(m: Incoming): Boolean = seen.add(m.tag).also { if (it) items.addLast(m) }
  @Synchronized fun poll(): Incoming? = items.removeFirstOrNull()
  @Synchronized fun clear() { items.clear(); seen.clear() }
  val size @Synchronized get() = items.size
}

enum class Answer { SEND, CANCEL, RETRY, UNKNOWN }

/** Interprète la réponse parlée à « envoyer, annuler ou recommencer ? » ; l'annulation l'emporte (« ne pas envoyer »). */
fun parseAnswer(heard: String): Answer {
  val s = Normalizer.normalize(heard.lowercase(), Normalizer.Form.NFD).replace(Regex("\\p{M}"), "")
  return when {
    Regex("annul|\\bnon\\b|\\bstop\\b|ne pas").containsMatchIn(s) -> Answer.CANCEL
    Regex("recommenc|refai|repet").containsMatchIn(s) -> Answer.RETRY
    Regex("envoi|envoy|\\boui\\b|\\bok\\b|valid").containsMatchIn(s) -> Answer.SEND
    else -> Answer.UNKNOWN
  }
}

/** Réglages du mode conduite (SharedPreferences « drive »), échangés avec le front en JSON. */
data class DriveSettings(
  val engine: String = "", // paquet du moteur de synthèse ; vide = celui du système
  val rate: Float = 1f,
  val readContent: Boolean = true,
  val confirm: Boolean = true, // false : envoi automatique après un court délai, sauf « annuler »
  val autoBluetooth: Boolean = false,
  val bluetooth: String = "", // adresse de l'appareil (la voiture)
) {
  /** Vitesse bornée : une valeur hors plage venant du front ne rend pas la voix inaudible. */
  val safeRate get() = rate.coerceIn(0.5f, 2f)
}
