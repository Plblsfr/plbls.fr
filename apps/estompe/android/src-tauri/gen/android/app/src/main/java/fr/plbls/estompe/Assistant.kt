package fr.plbls.estompe

import java.text.Normalizer

enum class Transport { PLAY, PAUSE, NEXT, PREVIOUS, VOLUME_UP, VOLUME_DOWN }

sealed class Command {
  object ReadUnread : Command()
  data class ReadFrom(val contact: String) : Command()
  object Reply : Command()
  data class SendSms(val contact: String, val text: String) : Command()
  data class Music(val action: Transport, val app: String) : Command()
  data class PlaySearch(val query: String, val app: String) : Command()
  data class PlayPlaylist(val name: String, val app: String) : Command()
  object Cancel : Command()
  object Help : Command()
  object Unknown : Command()
}

data class MusicApp(val id: String, val label: String)

data class Contact(val name: String, val numbers: List<String>)

data class Sms(val id: Long, val address: String, val sender: String, val body: String, val date: Long)

data class Heard(val source: String, val sender: String, val text: String)

enum class AssistantScope { OFF, DRIVE, ALWAYS }

data class AssistantSettings(
  val scope: AssistantScope = AssistantScope.OFF,
  val apps: Set<String> = emptySet(),
  val musicApp: String = "",
  val llm: Boolean = false,
)

const val WAKE_WORD = "estompe"

const val HELP_TEXT = "Vous pouvez dire : lis mes messages, lis les messages de Marie, envoie un SMS à Marie, mets Daft Punk, pause, suivant, ou lance ma playlist Route."

private val MARKS = Regex("\\p{M}")
private val NON_WORD = Regex("[^\\p{L}\\p{N}]+")

fun fold(text: String): String =
  Normalizer.normalize(text.lowercase().replace("œ", "oe").replace("æ", "ae"), Normalizer.Form.NFD)
    .replace(MARKS, "").replace(NON_WORD, " ").trim()

private fun count(folded: String) = folded.split(" ").count { it.isNotEmpty() }

private fun tail(original: String, folded: String): String {
  val k = count(folded)
  if (k == 0) return ""
  val out = ArrayList<String>()
  var n = 0
  for (w in original.trim().split(Regex("\\s+")).filter { fold(it).isNotEmpty() }.asReversed()) {
    val c = count(fold(w))
    if (n + c <= k) { out.add(w); n += c } else {
      val parts = Regex("[^'’-]+['’-]*").findAll(w).map { it.value }.filter { fold(it).isNotEmpty() }.toList()
      if (parts.size != c) return folded
      out.add(parts.takeLast(k - n).joinToString("")); n = k
    }
    if (n == k) break
  }
  return if (n == k) out.asReversed().joinToString(" ") else folded
}

private val WAKE_SINGLE = setOf("estompe", "estompes", "estompent", "estomper", "estompez", "estomp")
private val WAKE_PAIRS = setOf("est tombe", "est tomb", "est tombes", "est tompe", "et stomp", "et stompe")

fun wakeWord(heard: String): String? {
  val tokens = fold(heard).split(" ").filter { it.isNotEmpty() }
  var end = tokens.indexOfLast { it in WAKE_SINGLE } + 1
  if (tokens.size >= 2 && "${tokens[0]} ${tokens[1]}" in WAKE_PAIRS) end = maxOf(end, 2)
  return if (end == 0) null else tail(heard, tokens.drop(end).joinToString(" "))
}

private const val NOUN = "(?:musique|lecture|chanson|morceau|titre|piste)"
private const val MSG = "(?:messages?|sms|textos?)"
private const val NEW = "(?: (?:nouveaux|nouvelles|nouveau|nouvelle|derniers|dernieres|dernier|derniere))*"
private const val READ = "(?:lis|lire|lisez|lit)(?: moi)?"
private const val PLAY_VERBS = "mets|met|mettez|mettre|joue|jouer|jouez|lance|lancer|lancez|ecoute|ecouter|remets|remettre|play"

private val LEAD = Regex("^(?:(?:euh|alors|bon|donc|ok|hey|allez|please|merci|s il (?:te|vous) plait|stp|svp|" +
  "(?:est ce que )?(?:tu peux|vous pouvez|peux tu|pouvez vous|tu pourrais|vous pourriez|pourrais tu|pourriez vous)(?: me| moi)?|" +
  "est ce que|je voudrais|j aimerais|je veux|je souhaite|dis moi si|dis moi) )+")
private val TRAIL = Regex("(?: (?:merci|s il (?:te|vous) plait|stp|svp|please|maintenant))+$")

private val CANCEL = Regex("^(?:(?:non )?rien|non|annule|annuler|annulez|laisse tomber|laissez tomber|tant pis|oublie|oubliez)$")
private val HELP = Regex("^(?:aide|aide moi|que peux tu faire|que pouvez vous faire|que sais tu faire|que savez vous faire|" +
  "qu est ce que (?:tu|vous) (?:sais|savez|peux|pouvez) faire|tu sais faire quoi|tu peux faire quoi)$")
private val REPLY = Regex("^repond(?:s|re|ez)?(?: lui| leur| a (?:lui|elle|eux)| au message| a ce message)?$")
private val UNREAD = listOf(
  Regex("^$READ(?: (?:mes|les|tous mes|tous les))?$NEW $MSG(?: non lus?| recus?| recents?)?$"),
  Regex("^quels? (?:sont|est) (?:mes|les) (?:nouveaux |derniers )?$MSG(?: non lus?)?$"),
  Regex("^(?:mes (?:nouveaux |nouvelles |derniers )?|les (?:nouveaux|nouvelles|derniers) )$MSG(?: non lus?)?$"),
  Regex("^(?:j ai|ai je|y a t il|il y a) (?:recu )?(?:des|de|un) (?:nouveaux |nouvelles )?$MSG(?: non lus?)?$"),
)
private val READ_FROM = listOf(
  Regex("^$READ(?: (?:mes|les|le|la|tous les|tous mes))?$NEW $MSG(?: recus)? (?:de|du|d|venant de|envoyes? par|ecrits? par|par) (.+)$"),
  Regex("^(?:qu est ce que|qu est ce qu|que|qu) m a (?:ecrit|envoye|dit) (.+)$"),
  Regex("^qu est ce que (.+) m a (?:ecrit|envoye|dit)$"),
  Regex("^quels? (?:sont|est) (?:les|le) (?:nouveaux |derniers |dernier )?(?:messages?|sms) (?:de|du|d) (.+)$"),
  Regex("^(?:les|le dernier|le) (?:messages?|sms) (?:de|du|d) (.+)$"),
)
private val PRONOUNS = setOf("il", "elle", "lui", "ils", "elles", "on", "tu", "vous")
private val SEND = Regex("^(?:envoie|envoies|envoyer|envoyez|ecris|ecrire|ecrivez|dis|dire|dites)(?: (?:un|une))?(?: (?:petit|nouveau))?(?: (?:sms|message|texto))? (?:a|au) (.+)$")
private val MARKER = Regex(" (?:pour (?:lui |leur )?(?:dire|ecrire|annoncer)(?: que| qu)?|en (?:lui )?disant(?: que| qu)?|disant(?: que| qu)?|que|qu)(?= |$)")
private val APP_SUFFIX = Regex("^(.+) (?:sur|avec|dans|via) (.+)$")
private val TRANSPORTS = listOf(
  Transport.PAUSE to Regex("^(?:pause|stop|stoppe|stoppez|arrete|arretes|arretez|arreter|coupe|coupes|couper|silence|tais toi)(?:(?: (?:la|ma|de la|cette|ce|le))? (?:$NOUN|radio|son))?$"),
  Transport.PAUSE to Regex("^(?:mets|met|mettre|mettez|remets|fais|faire|faites)(?: (?:la|ma|le|cette) $NOUN)?(?: en)? pause(?: (?:la|ma|le|cette) $NOUN)?$"),
  Transport.NEXT to Regex("^(?:passe|passer|passez|saute|sauter|skip|next)(?: (?:la|le|cette|ce))?(?: $NOUN)?$"),
  Transport.NEXT to Regex("^(?:(?:mets|met|mettre|mettez|passe|passer|va|aller|lance|saute|sauter)(?: a)?(?: (?:la|le|au|cette|ce))? )?(?:$NOUN )?(?:suivante?|suivants?|prochaine?|prochains?)$"),
  Transport.PREVIOUS to Regex("^(?:(?:mets|met|mettre|mettez|va|aller|reviens|revenir|retourne|retourner|lance|joue|remets)(?: a)?(?: (?:la|le|au|cette|ce|l))? )?(?:$NOUN )?(?:precedente?|precedents?|d avant)$"),
  Transport.PREVIOUS to Regex("^(?:(?:reviens|revenir|retourne|retourner|recule|reculer) (?:en arriere|a la precedente)|retour|retour en arriere)$"),
  Transport.VOLUME_UP to Regex("^(?:mets |met |mettre )?(?:le son |la musique |le volume )?plus fort(?: un peu| encore)?$"),
  Transport.VOLUME_UP to Regex("^(?:monte|montes|monter|montez|augmente|augmentes|augmenter|augmentez|hausse|hausser)(?: un peu| encore)?(?: le| la)? (?:son|volume)(?: un peu| encore)?$"),
  Transport.VOLUME_UP to Regex("^plus de (?:son|volume)$"),
  Transport.VOLUME_DOWN to Regex("^(?:mets |met |mettre )?(?:le son |la musique |le volume )?(?:moins fort|plus bas|plus doucement)(?: un peu| encore)?$"),
  Transport.VOLUME_DOWN to Regex("^(?:baisse|baisses|baisser|baissez|diminue|diminues|diminuer|diminuez|descends|descendre|reduis|reduire)(?: un peu| encore)?(?: le| la)? (?:son|volume)(?: un peu| encore)?$"),
  Transport.VOLUME_DOWN to Regex("^moins de (?:son|volume)$"),
  Transport.PLAY to Regex("^(?:lecture|play|reprends|reprendre|reprise|continue|continuer|resume|(?:mets|met|mettre|mettez|joue|jouer|jouez|lance|lancer|lancez|relance|relancer|remets|remettre|reprends|reprendre|redemarre|redemarrer|demarre|demarrer|continue|continuer|allume|allumer)(?: (?:la|ma|de la|de ma|un peu de|le|cette|ta))? (?:musique|lecture|chanson|morceau))$"),
)
private val PLAYLIST = Regex("^(?:$PLAY_VERBS|demarre)(?: moi)? (?:ma |la |mon |une |ta |notre )?(?:playlist|liste de lecture)(?: nommee| intitulee| qui s appelle)?(?: (.+))?$")
private val SEARCH = Regex("^(?:$PLAY_VERBS)(?: moi)? (.+)$")
private val PLAYLIST_NAME_DET = Regex("^(?:de la |de l |du |de |d )")
private val SEARCH_DET = Regex("^(?:(?:un peu d(?:e|u|e la|e l|es) |un peu |de la |de l |du |des |de |d |" +
  "(?:la chanson|une chanson|le morceau|un morceau|le titre|un titre|l album|un album|la musique) (?:d(?:e|u|es) )?|" +
  "(?:la chanson|une chanson|le morceau|un morceau|le titre|un titre|l album|un album) |" +
  "l artiste |le groupe |le chanteur |la chanteuse |la |le |les |l )+)")
private val PREPOSITION = Regex("^(?:au|aux|a|avec|sur|dans|en|chez|pour|par|toi|te|nous|vous) ")
private val PLAYLIST_ONLY = Regex("^(?:ma |la |une )?(?:playlist|liste de lecture)$")

private fun appId(label: String, apps: List<MusicApp>): String? {
  val exact = apps.filter { fold(it.label) == label }
  if (exact.size == 1) return exact[0].id
  val words = label.split(" ")
  return apps.filter { a -> fold(a.label).split(" ").let { lw -> words.all { it in lw } } }.singleOrNull()?.id
}

private fun extractApp(c: String, apps: List<MusicApp>): Pair<String, String> {
  val m = APP_SUFFIX.find(c) ?: return c to ""
  val id = appId(m.groupValues[2].replace(Regex("^(?:l |la |le )?(?:application|appli|app) "), ""), apps) ?: return c to ""
  return m.groupValues[1] to id
}

private fun contactOf(s: String) = s.replace(TRAIL, "").replace(Regex(" (?:un|le) (?:sms|message|texto)$"), "")
  .replace(Regex("^(?:mon|ma) "), "").trim()

private fun sendSms(heard: String, lead: String): Command? {
  val rest = SEND.matchEntire(lead)?.groupValues?.get(1) ?: return null
  val m = MARKER.find(rest)
  val contact = contactOf(if (m == null) rest else rest.substring(0, m.range.first))
  if (contact.isEmpty()) return null
  val text = if (m == null) "" else tail(heard, rest.substring(m.range.last + 1).trim())
  return Command.SendSms(contact, text)
}

fun parseCommand(heard: String, apps: List<MusicApp> = emptyList()): Command {
  val lead = fold(heard).replace(LEAD, "").trim()
  val c = lead.replace(TRAIL, "").trim()
  if (c.isEmpty()) return Command.Unknown
  if (CANCEL.matches(c)) return Command.Cancel
  if (HELP.matches(c)) return Command.Help
  if (UNREAD.any { it.matches(c) }) return Command.ReadUnread
  READ_FROM.firstNotNullOfOrNull { r -> r.matchEntire(c)?.groupValues?.get(1)?.let(::contactOf)?.takeIf { it.isNotEmpty() && it !in PRONOUNS } }
    ?.let { return Command.ReadFrom(it) }
  if (REPLY.matches(c)) return Command.Reply
  sendSms(heard, lead)?.let { return it }
  val (core, app) = extractApp(c, apps)
  TRANSPORTS.firstOrNull { it.second.matches(core) }?.let { return Command.Music(it.first, app) }
  PLAYLIST.matchEntire(core)?.let { m ->
    val name = m.groups[1]?.value.orEmpty().replace(PLAYLIST_NAME_DET, "").trim()
    return if (name.isEmpty()) Command.Unknown else Command.PlayPlaylist(name, app)
  }
  val asked = SEARCH.matchEntire(core)?.groupValues?.get(1).orEmpty()
  val query = asked.replace(SEARCH_DET, "").trim()
  return if (query.isEmpty() || PREPOSITION.containsMatchIn(asked) || PLAYLIST_ONLY.matches(query)) Command.Unknown else Command.PlaySearch(query, app)
}

private fun distance(a: String, b: String): Int {
  var prev = IntArray(b.length + 1) { it }
  for (i in 1..a.length) {
    val cur = IntArray(b.length + 1)
    cur[0] = i
    for (j in 1..b.length) cur[j] = minOf(prev[j] + 1, cur[j - 1] + 1, prev[j - 1] + if (a[i - 1] == b[j - 1]) 0 else 1)
    prev = cur
  }
  return prev[b.length]
}

private fun closeness(word: String, name: List<String>): Int? {
  val best = name.map { it to distance(word, it) }.minByOrNull { it.second } ?: return null
  val len = minOf(word.length, best.first.length)
  return best.second.takeIf { it <= if (len <= 3) 0 else if (len <= 7) 1 else 2 }
}

fun matchContacts(spoken: String, contacts: List<Contact>): List<Contact> {
  val said = fold(spoken)
  if (said.isEmpty()) return emptyList()
  val words = said.split(" ")
  val book = contacts.filter { c -> c.numbers.any { it.isNotBlank() } }.map { it to fold(it.name) }
  book.filter { it.second == said }.let { r -> if (r.isNotEmpty()) return r.map { it.first } }
  book.filter { b -> words.all { it in b.second.split(" ") } }.let { r -> if (r.isNotEmpty()) return r.map { it.first } }
  val scored = book.mapNotNull { (c, n) ->
    val parts = n.split(" ")
    val d = words.map { closeness(it, parts) ?: return@mapNotNull null }
    c to d.sum()
  }
  val best = scored.minOfOrNull { it.second } ?: return emptyList()
  return scored.filter { it.second == best }.map { it.first }
}

fun samePhone(a: String, b: String): Boolean {
  if (a.any { it.isLetter() } || b.any { it.isLetter() }) return a.trim().equals(b.trim(), ignoreCase = true)
  val x = a.filter { it.isDigit() }
  val y = b.filter { it.isDigit() }
  return x.isNotEmpty() && (x == y || (x.length >= 9 && y.length >= 9 && x.takeLast(9) == y.takeLast(9)))
}

private val ORDINALS = listOf("premier premiere un une 1", "deuxieme second seconde deux 2", "troisieme trois 3", "quatrieme quatre 4",
  "cinquieme cinq 5", "sixieme six 6", "septieme sept 7", "huitieme huit 8", "neuvieme neuf 9", "dixieme dix 10").map { it.split(" ") }

fun pickChoice(heard: String, count: Int): Int? {
  val tokens = fold(heard).split(" ").filter { it.isNotEmpty() }
  if (tokens.isEmpty() || tokens.size > 5) return null
  val index = if (tokens.any { it == "dernier" || it == "derniere" }) count - 1 else tokens.firstNotNullOfOrNull { t -> ORDINALS.indexOfFirst { t in it }.takeIf { it >= 0 } }
  return index?.takeIf { it in 0 until count }
}

private fun ofName(name: String) = if (fold(name).firstOrNull() in "aeiou".toList()) "d'$name" else "de $name"

fun announceMessages(items: List<Heard>): List<String> {
  if (items.isEmpty()) return listOf("Aucun nouveau message.")
  val groups = ArrayList<MutableList<Pair<Heard, String>>>()
  for (h in items) {
    val t = spoken(h.text)
    val last = groups.lastOrNull()
    if (last != null && last[0].first.sender == h.sender && last[0].first.source == h.source && last[0].second.isEmpty() == t.isEmpty()) last.add(h to t)
    else groups.add(mutableListOf(h to t))
  }
  return groups.map { g ->
    val h = g[0].first
    val via = if (h.source.isBlank()) "" else " sur ${h.source}"
    val n = g.size
    when {
      g[0].second.isEmpty() -> "${h.sender} a envoyé ${if (n == 1) "un message" else "$n messages"} sans texte$via"
      n == 1 -> "Message ${ofName(h.sender)}$via : ${g[0].second}"
      else -> "$n messages ${ofName(h.sender)}$via : " + g.joinToString(" ") { (_, t) -> if (t.last() in ".!?…") t else "$t." }
    }
  }
}
