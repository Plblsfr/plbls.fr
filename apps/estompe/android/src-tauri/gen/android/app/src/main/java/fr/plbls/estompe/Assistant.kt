package fr.plbls.estompe

import java.text.Normalizer
import java.util.Calendar
import java.util.TimeZone

enum class Transport { PLAY, PAUSE, NEXT, PREVIOUS, VOLUME_UP, VOLUME_DOWN }

sealed class Command {
  object ReadUnread : Command()
  data class ReadFrom(val contact: String, val single: Boolean = false) : Command()
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

data class Heard(val source: String, val sender: String, val text: String, val time: Long = 0)

enum class AssistantScope { OFF, DRIVE, ALWAYS }

data class AssistantSettings(
  val scope: AssistantScope = AssistantScope.OFF,
  val apps: Set<String> = emptySet(),
  val musicApp: String = "",
  val llm: Boolean = false,
)

const val WAKE_WORD = "estompe"

const val HELP_TEXT = "Vous pouvez dire : lis mes messages, lis le dernier message de Marie, envoie un SMS à Marie, mets Daft Punk, lance YouTube Music, pause, suivant, ou lance ma playlist Route."

private val MARKS = Regex("\\p{M}")
private val NON_WORD = Regex("[^\\p{L}\\p{N}]+")

fun fold(text: String): String =
  Normalizer.normalize(text.lowercase().replace("œ", "oe").replace("æ", "ae"), Normalizer.Form.NFD)
    .replace(MARKS, "").replace(NON_WORD, " ").trim()

private fun count(folded: String) = folded.split(" ").count { it.isNotEmpty() }

private const val MAX_INPUT = 400

private fun bound(text: String) = if (text.length > MAX_INPUT) text.substring(0, MAX_INPUT) else text

private class Piece(val text: String, val tokens: Int, val spaced: Boolean)

private val SPACES = Regex("\\s+")
private val PIECE = Regex("[^'’-]+['’-]*")

private fun pieces(original: String): List<Piece> {
  val out = ArrayList<Piece>()
  for (w in original.trim().split(SPACES)) {
    var first = true
    for (m in PIECE.findAll(w)) { out.add(Piece(m.value, count(fold(m.value)), first)); first = false }
  }
  return out
}

private fun original(heard: String, folded: String, from: Int): String {
  val k = count(folded)
  if (k == 0) return ""
  val words = fold(heard).split(" ").filter { it.isNotEmpty() }
  val target = folded.split(" ").filter { it.isNotEmpty() }
  var s = -1
  for (i in from.coerceAtLeast(0)..words.size - k) if (words.subList(i, i + k) == target) { s = i; break }
  if (s < 0) return folded
  var n = 0
  val sb = StringBuilder()
  for (p in pieces(heard)) {
    if (p.tokens == 0) continue
    val a = n
    val b = n + p.tokens
    n = b
    if (b <= s || a >= s + k) continue
    if (a < s || b > s + k) return folded
    if (sb.isNotEmpty() && p.spaced) sb.append(' ')
    sb.append(p.text)
  }
  return sb.toString().replace(SPACES, " ").trim { !it.isLetterOrDigit() }.ifEmpty { folded }
}

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

private val WAKE_SINGLE = setOf("estompe", "estompes", "estomp")
private val WAKE_PAIRS = setOf("est tombe", "est tomb", "est tombes", "est tompe", "et stomp", "et stompe", "est tombee", "est tombees", "es tombe", "et tombe", "et tombee",
  "et tombes", "est tomber", "et tomber", "est tompes", "et tompe", "esse tombe")

fun wakeWord(input: String): String? {
  val heard = bound(input)
  val tokens = fold(heard).split(" ").filter { it.isNotEmpty() }
  var end = tokens.indexOfLast { it in WAKE_SINGLE } + 1
  if (tokens.size >= 2 && "${tokens[0]} ${tokens[1]}" in WAKE_PAIRS) end = maxOf(end, 2)
  return if (end == 0) null else tail(heard, tokens.drop(end).joinToString(" "))
}

val WAKE_DECOYS: List<String> = ("le la les un une des du de d et est on il elle ils elles je j tu nous vous ce c ça se s ne n pas plus que qu qui quoi où quand " +
  "comment pourquoi oui non si mais ou donc car à au aux en dans sur sous avec sans pour par chez vers entre avant après pendant depuis mon ma mes ton ta tes son sa ses " +
  "notre votre leur leurs me te lui y a ai as avons avez ont suis es sommes êtes sont été être avoir fait faire fais dit dire dis va vais vas aller allez vient venir " +
  "peux peut pouvez veux veut voulez dois doit faut sais sait vu voir vois pris prendre mis mettre mets passe passer reste rester donne donner parle parler pense penser " +
  "trouve trouver arrive arriver tout tous toute toutes très bien bon bonne mal peu beaucoup trop assez encore déjà toujours jamais aussi alors puis ensuite ici là " +
  "maintenant aujourd'hui hier demain matin soir nuit jour heure heures minute minutes temps fois an ans année semaine mois maison voiture route train ville rue travail " +
  "école enfant enfants homme femme ami amis père mère frère soeur chose choses monde vie main tête eau pain café restaurant manger boire dormir acheter grand petit " +
  "nouveau vieux beau premier dernier prochain autre même deux trois quatre cinq six sept huit neuf dix vingt cent mille gauche droite haut bas devant derrière merci " +
  "bonjour salut pardon attention regarde écoute attends stop estomac tombe tombé tomber instant message messages musique téléphone problème question réponse idée " +
  "moment côté part cas point place nom numéro").split(" ")

val FIXED_ORDERS: List<String> = listOf("pause", "stop", "suivant", "suivante", "précédent", "chanson suivante", "chanson précédente", "plus fort", "moins fort",
  "monte le son", "baisse le son", "monte le volume", "baisse le volume", "lis mes messages", "lis les messages", "lis mes nouveaux messages", "est-ce que j'ai des messages",
  "réponds", "aide", "annule", "reprends", "reprends la musique", "lecture", "mets la musique", "mets de la musique", "arrête la musique")

val CONFIRM_PHRASES: Set<String> = setOf("envoyer", "envoie", "oui envoie", "oui envoyer", "ok envoie", "vas-y envoie", "annuler", "annule", "non", "stop", "recommencer", "recommence")

val CHOICE_PHRASES: Set<String> = setOf("un", "deux", "trois", "quatre", "le premier", "le deuxième", "le troisième", "le quatrième", "la première", "la deuxième",
  "la troisième", "la quatrième", "le dernier", "la dernière", "annuler", "annule")

private const val PHRASE_NAMES = 200
private val NOT_SPOKEN = Regex("[^\\p{L}'’ -]+")

fun grammarWords(text: String): String = text.lowercase().replace('’', '\'').replace(NOT_SPOKEN, " ").replace(SPACES, " ").trim()

fun grammarJson(phrases: Collection<String>): String =
  (phrases + "[unk]").joinToString(",", "[", "]") { "\"" + it.replace("\\", "\\\\").replace("\"", "\\\"") + "\"" }

fun wakeGrammar(): String = grammarJson(listOf(WAKE_WORD) + WAKE_DECOYS)

fun commandPhrases(contacts: List<String>, apps: List<String>): Set<String> {
  val out = LinkedHashSet<String>(FIXED_ORDERS)
  val names = LinkedHashSet<String>()
  for (c in contacts) {
    val full = grammarWords(c)
    if (full.isEmpty()) continue
    names.add(full)
    names.add(full.substringBefore(' '))
    if (names.size >= PHRASE_NAMES) break
  }
  for (n in names) {
    val of = if (fold(n).firstOrNull() in "aeiouh".toList()) "d'$n" else "de $n"
    out.add("lis les messages $of")
    out.add("lis le dernier message $of")
    out.add("lis le dernier sms $of")
    out.add("lis les sms $of")
    out.add("envoie un sms à $n")
    out.add("envoie un message à $n")
  }
  for (a in apps) for (label in setOf(grammarWords(a), appKey(a))) {
    if (label.isEmpty()) continue
    out.add("lance $label")
    out.add("ouvre $label")
  }
  return out
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

private val CANCEL = Regex("^(?:je ne veux plus|pas du tout|non|(?:non )?(?:rien|annule|annuler|annulez|laisse tomber|laissez tomber|tant pis|oublie|oubliez)(?: [a-z0-9]+){0,2})$")
private val HELP = Regex("^(?:aide|aide moi|que peux tu faire|que pouvez vous faire|que sais tu faire|que savez vous faire|" +
  "qu est ce que (?:tu|vous) (?:sais|savez|peux|pouvez) faire|tu sais faire quoi|tu peux faire quoi)$")
private val REPLY = Regex("^repond(?:s|re|ez)?(?: lui| leur| a (?:lui|elle|eux)| au message| a ce message)?$")
private val UNREAD = listOf(
  Regex("^$READ(?: (?:mes|les|tous mes|tous les))?$NEW $MSG(?: non lus?| recus?| recents?)?$"),
  Regex("^quels? (?:sont|est) (?:mes|les) (?:nouveaux |derniers )?$MSG(?: non lus?)?$"),
  Regex("^(?:mes (?:nouveaux |nouvelles |derniers )?|les (?:nouveaux|nouvelles|derniers) )$MSG(?: non lus?)?$"),
)
private val UNREAD_ASK = Regex("^(?:(?:euh|alors|bon|donc|ok|hey|allez) )*(?:est ce que? (?:j ai|il y a)|ai je|y a t il)(?: recu)? (?:des|de|un) (?:nouveaux |nouvelles )?$MSG(?: non lus?)?$")
private val READ_FROM = listOf(
  Regex("^$READ(?: (?:mes|les|le|la|tous les|tous mes))?$NEW $MSG(?: recus)? (?:de|du|d|venant de|envoyes? par|ecrits? par|par) (.+)$"),
  Regex("^(?:qu est ce que|qu est ce qu|que|qu) m a (?:ecrit|envoye|dit) (.+)$"),
  Regex("^qu est ce que (.+) m a (?:ecrit|envoye|dit)$"),
  Regex("^quels? (?:sont|est) (?:les|le) (?:nouveaux |derniers |dernier )?(?:messages?|sms) (?:de|du|d) (.+)$"),
  Regex("^(?:les |le dernier |les derniers |le |dernier |derniers )?(?:messages?|sms|textos?) (?:de|du|d) (.+)$"),
)
private val PRONOUNS = setOf("il", "elle", "lui", "ils", "elles", "on", "tu", "vous")
private val SEND = Regex("^(?:envoie|envoies|envoyer|envoyez|ecris|ecrire|ecrivez|dis|dire|dites)(?: (?:un|une))?(?: (?:petit|nouveau))?(?: (?:sms|message|texto))? (?:a|au) (.+)$")
private val MARKER = Regex(" (?:pour (?:lui |leur )?(?:dire|ecrire|annoncer)(?: que| qu)?|en (?:lui )?disant(?: que| qu)?|disant(?: que| qu)?|que|qu)(?= |$)")
private val ON_APP = Regex(" sur .+$")
private val APP_SUFFIX = Regex("^(.+) (?:sur|avec|dans|via) (.+)$")
private val TRANSPORTS = listOf(
  Transport.PAUSE to Regex("^(?:pause|stop|stoppe|stoppez|arrete|arretes|arretez|arreter|coupe|coupes|couper|silence|tais toi)(?:(?: (?:la|ma|de la|cette|ce|le))? (?:$NOUN|radio|son))?$"),
  Transport.PAUSE to Regex("^(?:mets|met|mettre|mettez|remets|fais|faire|faites)(?: (?:la|ma|le|cette) $NOUN| le| la)?(?: en| sur)? pause(?: (?:la|ma|le|cette) $NOUN)?$"),
  Transport.NEXT to Regex("^(?:passe|passer|passez|saute|sauter|skip|next)(?: (?:la|le|cette|ce))?(?: $NOUN)?$"),
  Transport.NEXT to Regex("^(?:(?:mets|met|mettre|mettez|passe|passer|va|aller|lance|saute|sauter)(?: a)?(?: (?:la|le|au|cette|ce))? )?(?:$NOUN )?(?:suivante?|suivants?|prochaine?|prochains?)$"),
  Transport.PREVIOUS to Regex("^(?:(?:mets|met|mettre|mettez|va|aller|reviens|revenir|retourne|retourner|lance|joue|remets)(?: a)?(?: (?:la|le|au|cette|ce|l))? )?(?:$NOUN )?(?:precedente?|precedents?|d avant)$"),
  Transport.PREVIOUS to Regex("^(?:(?:reviens|revenir|retourne|retourner|recule|reculer) (?:en arriere|a la precedente)|retour|retour en arriere)$"),
  Transport.VOLUME_UP to Regex("^(?:mets |met |mettre )?(?:le son |la musique |le volume )?plus fort(?: un peu| encore)?$"),
  Transport.VOLUME_UP to Regex("^(?:monte|montes|monter|montez|augmente|augmentes|augmenter|augmentez|hausse|hausser)(?: un peu| encore)?(?: le| la)? (?:son|volume)(?: un peu| encore)?$"),
  Transport.VOLUME_UP to Regex("^plus de (?:son|volume)$"),
  Transport.VOLUME_UP to Regex("^(?:(?:mets|met|mettre|mettez|monte|monter|montez|augmente|augmenter|augmentez) )?(?:(?:le|la) )?(?:volume|son|musique) (?:a fond|au max|au maximum|a bloc)$"),
  Transport.VOLUME_UP to Regex("^(?:monte|monter|montez|augmente|augmenter|augmentez)(?: un peu| encore)$"),
  Transport.VOLUME_DOWN to Regex("^(?:mets |met |mettre )?(?:le son |la musique |le volume )?(?:moins fort|plus bas|plus doucement)(?: un peu| encore)?$"),
  Transport.VOLUME_DOWN to Regex("^(?:baisse|baisses|baisser|baissez|diminue|diminues|diminuer|diminuez|descends|descendre|reduis|reduire)(?: un peu| encore)?(?: le| la)? (?:son|volume)(?: un peu| encore)?$"),
  Transport.VOLUME_DOWN to Regex("^moins de (?:son|volume)$"),
  Transport.VOLUME_DOWN to Regex("^(?:baisse|baisses|baisser|baissez|diminue|diminuer|diminuez)(?: un peu| encore)?$"),
  Transport.PLAY to Regex("^(?:lecture|play|reprends|reprendre|reprise|continue|continuer|resume|(?:mets|met|mettre|mettez|joue|jouer|jouez|ecoute|ecouter|ecoutez|lance|lancer|lancez|relance|relancer|remets|remettre|reprends|reprendre|redemarre|redemarrer|demarre|demarrer|continue|continuer|allume|allumer)(?: (?:la|ma|de la|de ma|un peu de|le|cette|ta))? (?:musique|lecture|chanson|morceau))$"),
)
private val PLAYLIST = Regex("^(?:$PLAY_VERBS|demarre)(?: moi)? (?:ma |la |mon |une |ta |notre )?(?:playlist|liste de lecture)(?: nommee| intitulee| qui s appelle)?(?: (.+))?$")
private val SEARCH = Regex("^($PLAY_VERBS)(?: moi)? (.+)$")
private val PLAYLIST_NAME_DET = Regex("^(?:de la |de l |du |de |d )")
private val SEARCH_DET = Regex("^(?:(?:un peu d(?:e|u|e la|e l|es) |un peu |de la |de l |du |des |de |d |" +
  "(?:la chanson|une chanson|le morceau|un morceau|le titre|un titre|l album|un album|la musique) (?:d(?:e|u|es) )?|" +
  "(?:la chanson|une chanson|le morceau|un morceau|le titre|un titre|l album|un album) |" +
  "l artiste |le groupe |le chanteur |la chanteuse |la |le |les |l )+)")
private val PREPOSITION = Regex("^(?:au|aux|a|en|chez|pour|par|toi|te|nous|vous) |^(?:sur|dans|avec) (?:(?:la|le|l|ma|mon|ta|ton|cette|ce) )?(?:pause|stop|lecture|silence)(?: |$)")
private val AMBIGUOUS_VERBS = setOf("ecoute", "ecouter", "lance", "lancer", "lancez")
private val DETERMINERS = setOf("un", "une", "cet", "cette")
private val SEARCH_STOP = setOf("moi", "toi", "lui", "ca", "ce", "cela", "ceci", "celui", "celle", "le", "la", "les", "l", "bien", "tout", "tous", "toute", "toutes",
  "quelque", "chose", "quelquechose", "attentivement", "un", "une", "des", "du", "de", "d", "y", "en", "encore", "aussi", "musique", "chanson", "morceau", "titre", "album", "artiste", "groupe")
private val SEARCH_PRONOUN_FIRST = setOf("moi", "toi", "lui", "ca", "ce", "cela", "ceci", "celui", "celle")
private val SEARCH_EXCLUDE = setOf("lumiere", "lumieres", "lampe", "feu", "couvert", "couverts", "table", "tele", "television", "minuteur", "minuterie", "timer", "chrono", "chronometre",
  "reveil", "alarme", "chauffage", "clim", "climatisation", "application", "appli", "app", "machine", "manteau", "veste", "pull", "pantalon", "chaussures", "chapeau", "linge", "vaisselle",
  "volume", "son", "stop", "pause", "message", "messages", "sms", "texto", "textos", "mes", "mon", "ton", "ta", "tes", "ma", "que", "qui", "qu", "porte", "fenetre", "telephone", "portable",
  "ecran", "camera", "gps", "navigation", "itineraire", "wifi", "bluetooth", "mode", "calendrier", "agenda", "rappel", "cafe", "the", "eau", "ordinateur")
private val OPEN = Regex("^(?:$PLAY_VERBS|ouvre|ouvrir|ouvrez|demarre|demarrer|demarrez)(?: moi)? (?:(?:l |la |le )?(?:application|appli|app) )?(.+)$")
private val YOUTUBE = Regex("(?:^| )(?:you ?tube?|you ?tub|yt)(?= |$)")
private val MUSIC_WORD = Regex("(?:^| )(?:musique|musiques|musik|musics)(?= |$)")
private val PLAYLIST_ONLY = Regex("^(?:ma |la |une )?(?:playlist|liste de lecture)$")

fun appKey(label: String): String = fold(label).replace(YOUTUBE, " youtube").replace(MUSIC_WORD, " music").trim()

private fun appId(label: String, apps: List<MusicApp>): String? {
  val said = appKey(label)
  if (said.isEmpty()) return null
  val keyed = apps.map { it to appKey(it.label) }
  val exact = keyed.filter { it.second == said || it.second.replace(" ", "") == said.replace(" ", "") }
  if (exact.size == 1) return exact[0].first.id
  val words = said.split(" ")
  return keyed.filter { (_, k) -> k.split(" ").let { lw -> words.all { it in lw } } }.singleOrNull()?.first?.id
}

private fun extractApp(c: String, apps: List<MusicApp>): Pair<String, String> {
  val m = APP_SUFFIX.find(c) ?: return c to ""
  val id = appId(m.groupValues[2].replace(Regex("^(?:l |la |le )?(?:application|appli|app) "), ""), apps) ?: return c to ""
  return m.groupValues[1] to id
}

private fun musicLike(verb: String, asked: String, query: String): Boolean {
  if (query.isEmpty() || PREPOSITION.containsMatchIn(asked) || PLAYLIST_ONLY.matches(query)) return false
  val words = query.split(" ")
  val first = words.dropWhile { it in DETERMINERS }.firstOrNull() ?: return false
  if (first in SEARCH_EXCLUDE || first in SEARCH_PRONOUN_FIRST) return false
  if (words.any { it == "message" || it == "messages" || it == "sms" }) return false
  if (words.all { it in SEARCH_STOP }) return false
  if (verb in AMBIGUOUS_VERBS && words.none { it !in SEARCH_STOP && it !in SEARCH_EXCLUDE }) return false
  return true
}

private fun contactOf(s: String) = s.replace(TRAIL, "").replace(Regex(" (?:un|le) (?:sms|message|texto)$"), "")
  .replace(Regex("^(?:mon|ma) "), "").trim()

private fun sendSms(heard: String, lead: String, base: Int): Command? {
  val g = SEND.matchEntire(lead)?.groups?.get(1) ?: return null
  val rest = g.value
  val m = MARKER.find(rest)
  val folded = contactOf(if (m == null) rest else rest.substring(0, m.range.first))
  if (folded.isEmpty()) return null
  val contact = original(heard, folded, base + count(lead.substring(0, g.range.first)))
  val text = if (m == null) "" else tail(heard, rest.substring(m.range.last + 1).trim())
  return Command.SendSms(contact, text)
}

private val REPAIRS = listOf(
  "mets" to setOf("mais", "mes", "mai", "met", "m es", "mettent"),
  "lance" to setOf("l ancien", "l ancienne", "lancer", "lances", "lancent", "l anse", "lancement"),
  "lis" to setOf("ii", "lit", "li", "lie", "lise", "lies", "lu", "il"),
  "joue" to setOf("jour", "jo", "joues", "jouent", "joux"),
  "ouvre" to setOf("ouvres", "ouvrent", "ouvert"),
  "envoie" to setOf("envoi", "envoient", "envois"),
  "dis" to setOf("dit", "dits"),
  "reprends" to setOf("reprend"),
  "pause" to setOf("pose", "poses", "posait", "poser", "pauses"),
)

private val REPAIR_STOP = setOf("non", "oui", "si", "bon", "enfin", "alors", "quoi", "bien", "ok", "pourquoi", "comment", "attends", "merci", "voila", "c", "est", "ca", "pas",
  "mal", "vrai", "qu", "que", "il", "elle", "je", "tu", "on", "ne", "sais", "moi", "toi", "ou", "et", "donc", "quand", "meme")

private val SMS_HEARD = Regex("(?i)(?<![\\p{L}])(?:(?:essaie|essaies|essai|essaye|esse|est-ce|c'est|ses|ces|s|se) (?:messe|messes|mes|mais|aime s|m s|emme esse)|messes?|cms|s m s)(?![\\p{L}])")

private fun repair(heard: String): String? {
  val sms = heard.replace(SMS_HEARD, "sms")
  val words = sms.trim().split(SPACES).filter { it.isNotEmpty() }
  val first = fold(words.firstOrNull() ?: return null)
  val to = REPAIRS.firstOrNull { first in it.second }?.first
  if (to == null) return sms.takeIf { it != heard }
  return (listOf(to) + words.drop(1)).joinToString(" ")
}

fun parseCommand(input: String, apps: List<MusicApp> = emptyList()): Command {
  val c = parseExact(input, apps)
  if (c != Command.Unknown) return c
  val fixed = parseExact(repair(bound(input)) ?: return c, apps)
  return if (fixed is Command.PlaySearch && fold(fixed.query).split(" ").all { it in REPAIR_STOP }) c else fixed
}

private fun parseExact(input: String, apps: List<MusicApp>): Command {
  val heard = bound(input)
  val all = fold(heard)
  val lead = all.replace(LEAD, "").trim()
  val c = lead.replace(TRAIL, "").trim()
  if (c.isEmpty()) return Command.Unknown
  if (CANCEL.matches(c)) return Command.Cancel
  if (HELP.matches(c)) return Command.Help
  if (UNREAD.any { it.matches(c) } || UNREAD_ASK.matches(all.replace(TRAIL, "").trim())) return Command.ReadUnread
  val base = count(all) - count(lead)
  for (r in READ_FROM) {
    val g = r.matchEntire(c)?.groups?.get(1) ?: continue
    val name = contactOf(g.value).replace(ON_APP, "").trim()
    if (name.isEmpty() || name in PRONOUNS) continue
    val before = c.substring(0, g.range.first).split(" ")
    val single = "message" in before || "texto" in before || ("sms" in before && ("le" in before || "dernier" in before))
    return Command.ReadFrom(original(heard, name, base + count(c.substring(0, g.range.first))), single)
  }
  if (REPLY.matches(c)) return Command.Reply
  sendSms(heard, lead, base)?.let { return it }
  val (core, app) = extractApp(c, apps)
  TRANSPORTS.firstOrNull { it.second.matches(core) }?.let { return Command.Music(it.first, app) }
  PLAYLIST.matchEntire(core)?.let { m ->
    val name = m.groups[1]?.value.orEmpty().replace(PLAYLIST_NAME_DET, "").trim()
    return if (name.isEmpty()) Command.Unknown else Command.PlayPlaylist(name, app)
  }
  OPEN.matchEntire(c)?.let { o -> appId(o.groupValues[1], apps)?.let { return Command.Music(Transport.PLAY, it) } }
  val m = SEARCH.matchEntire(core) ?: return Command.Unknown
  val asked = m.groupValues[2]
  val query = asked.replace(SEARCH_DET, "").trim()
  return if (musicLike(m.groupValues[1], asked, query)) Command.PlaySearch(query, app) else Command.Unknown
}

fun splitRecipient(spoken: String, contacts: List<Contact>): Pair<List<Contact>, String> {
  val whole = matchContacts(spoken, contacts)
  if (whole.isNotEmpty()) return whole to ""
  val words = spoken.trim().split(SPACES).filter { it.isNotEmpty() }
  for (n in minOf(3, words.size - 1) downTo 1) {
    val found = matchContacts(words.take(n).joinToString(" "), contacts)
    if (found.isNotEmpty()) return found to words.drop(n).joinToString(" ")
  }
  return emptyList<Contact>() to ""
}

private val CANCEL_PARTS = listOf("annul", "arrete", "oublie", "stop", "laisse tomber", "tant pis", "plus tard")
private val CANCEL_WORDS = setOf("non", "ne", "n", "pas", "rien", "jamais", "surtout", "attends", "attendez")
private val RETRY_PARTS = listOf("recommenc", "refai", "repet", "corrige")
private val RETRY_WORDS = setOf("redi", "redis", "redites")
private val SEND_WORDS = setOf("envoie", "envoyer", "envoyez", "envoi")
private val CONFIRM_WORDS = SEND_WORDS + setOf("oui", "ok", "d", "accord", "vas", "y", "allez", "le", "l", "la", "tu", "peux", "vous", "pouvez", "s", "il", "te", "plait", "merci")

fun confirmAnswer(heard: String): Answer {
  val f = fold(bound(heard))
  val w = f.split(" ").filter { it.isNotEmpty() }
  return when {
    CANCEL_PARTS.any { it in f } || w.any { it in CANCEL_WORDS } -> Answer.CANCEL
    RETRY_PARTS.any { it in f } || w.any { it in RETRY_WORDS } -> Answer.RETRY
    w.size in 1..4 && w.any { it in SEND_WORDS } && w.all { it in CONFIRM_WORDS } -> Answer.SEND
    else -> Answer.UNKNOWN
  }
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

private val CHOICE_REJECT = setOf("pas", "ne", "n", "non", "attends", "attendez", "attend")

fun pickChoice(heard: String, count: Int): Int? {
  val tokens = fold(bound(heard)).split(" ").filter { it.isNotEmpty() }
  if (tokens.isEmpty() || tokens.size > 3 || tokens.any { it in CHOICE_REJECT }) return null
  if (tokens.indices.any { it + 1 < tokens.size && tokens[it] in setOf("un", "une") && tokens[it + 1] in setOf("seconde", "second") }) return null
  val index = if (tokens.any { it == "dernier" || it == "derniere" }) count - 1
    else tokens.firstNotNullOfOrNull { t -> if (t in setOf("un", "une") && tokens.size > 2) null else ORDINALS.indexOfFirst { t in it }.takeIf { it >= 0 } }
  return index?.takeIf { it in 0 until count }
}

private fun ofName(name: String) = if (fold(name).firstOrNull() in "aeiou".toList()) "d'$name" else "de $name"

private val DAYS = listOf("dimanche", "lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi")
private val MONTHS = listOf("janvier", "février", "mars", "avril", "mai", "juin", "juillet", "août", "septembre", "octobre", "novembre", "décembre")
private const val DAY_MS = 86_400_000L

fun whenText(time: Long, now: Long, zone: TimeZone = TimeZone.getDefault()): String {
  if (time <= 0 || now <= 0) return ""
  val ago = now - time
  if (ago < 120_000) return "à l'instant"
  if (ago < 3_600_000) return "il y a ${ago / 60_000} minutes"
  val t = Calendar.getInstance(zone).apply { timeInMillis = time }
  val n = Calendar.getInstance(zone).apply { timeInMillis = now }
  val minute = t.get(Calendar.MINUTE)
  val hour = "à ${t.get(Calendar.HOUR_OF_DAY)} h" + if (minute == 0) "" else " " + minute.toString().padStart(2, '0')
  val days = Math.floorDiv(now + zone.getOffset(now), DAY_MS) - Math.floorDiv(time + zone.getOffset(time), DAY_MS)
  if (days == 0L) return hour
  if (days == 1L) return "hier $hour"
  if (days in 2..6) return "${DAYS[t.get(Calendar.DAY_OF_WEEK) - 1]} $hour"
  val day = t.get(Calendar.DAY_OF_MONTH).let { if (it == 1) "1er" else it.toString() }
  val year = if (t.get(Calendar.YEAR) == n.get(Calendar.YEAR)) "" else " ${t.get(Calendar.YEAR)}"
  return "le $day ${MONTHS[t.get(Calendar.MONTH)]}$year"
}

fun announceMessages(items: List<Heard>, now: Long = 0, zone: TimeZone = TimeZone.getDefault()): List<String> {
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
    val sms = h.source.isBlank()
    val via = if (sms) "" else " sur ${h.source}"
    val n = g.size
    val at = whenText(g.last().first.time, now, zone)
    val date = if (at.isEmpty()) "" else if (n == 1) ", $at" else ", le dernier $at"
    when {
      g[0].second.isEmpty() -> "${h.sender} a envoyé ${if (n == 1) (if (sms) "un SMS" else "un message") else "$n ${if (sms) "SMS" else "messages"}"} sans texte$via$date"
      n == 1 -> "${if (sms) "SMS" else "Message"} ${ofName(h.sender)}$via$date : ${g[0].second}"
      else -> "$n ${if (sms) "SMS" else "messages"} ${ofName(h.sender)}$via$date : " + g.joinToString(" ") { (_, t) -> if (t.last() in ".!?…") t else "$t." }
    }
  }
}
