package fr.plbls.estompe

import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class AssistantServiceTest {
  private val current = AssistantSettings(AssistantScope.DRIVE, setOf("org.signal"), "com.music", true)

  @Test fun `portée lue sans tenir compte de la casse, inconnue ignorée`() {
    assertEquals(AssistantScope.ALWAYS, scopeOf("always"))
    assertEquals(AssistantScope.OFF, scopeOf(" OFF "))
    assertNull(scopeOf("parfois"))
    assertNull(scopeOf(3))
    assertNull(scopeOf(null))
  }

  @Test fun `réglages JSON, champs absents ou mal typés conservés`() {
    assertEquals(current, settingsFrom(JSONObject(), current))
    assertEquals(current, settingsFrom(JSONObject().put("scope", "jamais").put("apps", "x").put("musicApp", 4).put("llm", "oui"), current))
    val s = settingsFrom(JSONObject().put("scope", "always").put("apps", JSONArray().put("a").put(2).put(" ").put("b")).put("musicApp", "").put("llm", false), current)
    assertEquals(AssistantSettings(AssistantScope.ALWAYS, setOf("a", "b"), "", false), s)
  }

  @Test fun `réglages exportés en JSON`() {
    val j = settingsJson(AssistantSettings(AssistantScope.DRIVE, setOf("b", "a"), "m", true))
    assertEquals("drive", j.getString("scope"))
    assertEquals(listOf("a", "b"), (0 until j.getJSONArray("apps").length()).map { j.getJSONArray("apps").getString(it) })
    assertEquals("m", j.getString("musicApp"))
    assertTrue(j.getBoolean("llm"))
    assertEquals(current, settingsFrom(settingsJson(current), AssistantSettings()))
  }

  @Test fun `le service tourne selon la portée et le mode conduite`() {
    assertFalse(shouldRun(AssistantSettings(AssistantScope.OFF), true))
    assertTrue(shouldRun(AssistantSettings(AssistantScope.ALWAYS), false))
    assertTrue(shouldRun(AssistantSettings(AssistantScope.DRIVE), true))
    assertFalse(shouldRun(AssistantSettings(AssistantScope.DRIVE), false))
  }

  @Test fun `SMS et notifications fusionnés par date, notifications déjà lues écartées`() {
    val sms = listOf(Sms(1, "0611", "Marie", "salut", 100), Sms(2, "0622", "Paul", "ok", 300))
    val notes = listOf(Pending("k1", "org.signal", "Signal", "Léa", "coucou", 200, true), Pending("k2", "org.signal", "Signal", "Léa", "vu", 50, false))
    val items = mergeUnread(sms, notes, setOf("k2"))
    assertEquals(listOf(Heard("", "Marie", "salut", 100), Heard("Signal", "Léa", "coucou", 200), Heard("", "Paul", "ok", 300)), items.map { it.heard })
    assertEquals(ReplyTo.Text("0622", "Paul"), items.last().to)
    assertEquals(ReplyTo.Note("k1", "Léa", "Signal", true), items[1].to)
    assertTrue(mergeUnread(emptyList(), notes, setOf("k1", "k2")).isEmpty())
  }

  @Test fun `à date égale, le SMS passe avant la notification`() {
    val items = mergeUnread(listOf(Sms(1, "06", "A", "x", 10)), listOf(Pending("k", "p", "App", "B", "y", 10, false)), emptySet())
    assertEquals(listOf("A", "B"), items.map { it.heard.sender })
  }

  @Test fun `choix énoncés, quatre au plus`() {
    assertEquals("Plusieurs contacts : un, Marie Dupont ; deux, Marie Martin. Lequel ?", choices("Plusieurs contacts", listOf("Marie Dupont", "Marie Martin")))
    assertEquals("X : un, a ; deux, b ; trois, c ; quatre, d. Lequel ?", choices("X", listOf("a", "b", "c", "d")))
  }

  @Test fun `liste tronquée annoncée`() {
    assertEquals("X : un, a ; deux, b ; trois, c ; quatre, d ; et 1 autre. Lequel ?", choices("X", listOf("a", "b", "c", "d", "e")))
    assertEquals("Plusieurs contacts : un, a ; deux, b ; trois, c ; quatre, d ; et 3 autres, précisez le nom. Lequel ?",
      choices("Plusieurs contacts", listOf("a", "b", "c", "d", "e", "f", "g"), "précisez le nom"))
  }

  @Test fun `confirmation, jamais d'envoi sans accord explicite`() {
    assertEquals(Confirm.SEND, confirmStep(confirmAnswer("envoyer"), 1))
    assertEquals(Confirm.RETRY, confirmStep(confirmAnswer("recommencer"), 1))
    assertEquals(Confirm.CANCEL, confirmStep(confirmAnswer("annuler"), 1))
    assertEquals(Confirm.CANCEL, confirmStep(confirmAnswer("ne pas envoyer"), 1))
    assertEquals(Confirm.ASK, confirmStep(confirmAnswer("oui"), 1))
    assertEquals(Confirm.ASK, confirmStep(confirmAnswer(""), 1))
    assertEquals(Confirm.ASK, confirmStep(confirmAnswer("bof"), 1))
    assertEquals(Confirm.CANCEL, confirmStep(confirmAnswer(""), 2))
    assertEquals(Confirm.CANCEL, confirmStep(confirmAnswer("hein"), 2))
    for (a in Answer.values()) for (n in 1..3) if (a != Answer.SEND) assertFalse(confirmStep(a, n) == Confirm.SEND)
  }

  @Test fun `relecture du message avant envoi`() {
    assertEquals("SMS pour Marie : j'arrive. Envoyer, annuler ou recommencer ?", review(ReplyTo.Text("06", "Marie"), "j'arrive"))
    assertEquals("Réponse à Léa sur Signal : oui. Envoyer, annuler ou recommencer ?", review(ReplyTo.Note("k", "Léa", "Signal", true), "oui"))
  }

  @Test fun `numéro relu quand le contact en a plusieurs`() {
    assertEquals("SMS pour Marie, au 06 12 34 56 78 : j'arrive. Envoyer, annuler ou recommencer ?", review(ReplyTo.Text("+33612345678", "Marie", true), "j'arrive"))
  }

  @Test fun `SMS déjà lus par sa notification écartés`() {
    val sms = listOf(Sms(1, "+33611223344", "Marie", "J'arrive !", 100))
    val notes = listOf(Pending("k1", "sms.app", "Messages", "marie", "j'arrive", 101, true), Pending("k2", "sms.app", "Messages", "06 11 22 33 44", "J'arrive !", 102, true),
      Pending("k3", "sms.app", "Messages", "Marie", "autre chose", 103, true), Pending("k4", "org.signal", "Signal", "Paul", "j'arrive", 104, true))
    val items = mergeUnread(sms, notes, emptySet())
    assertEquals(listOf("Marie", "Marie", "Paul"), items.map { it.heard.sender })
    assertEquals(listOf("", "Messages", "Signal"), items.map { it.heard.source })
    assertEquals(listOf("k1", "k2"), items[0].notes)
    assertEquals(1L, items[0].sms)
  }

  @Test fun `lecture plafonnée et notifications pas encore accessibles`() {
    val h = listOf(Heard("", "Marie", "salut"))
    assertEquals(listOf("SMS de Marie : salut", "Il en reste 3."), unreadLines(h, 3, false, true))
    assertEquals(listOf("SMS de Marie : salut"), unreadLines(h, 0, false, true))
    assertEquals(listOf("Aucun nouveau message."), unreadLines(emptyList(), 0, false, true))
    assertEquals(listOf("Aucun nouveau SMS.", NOTES_MISSING), unreadLines(emptyList(), 0, true, true))
    assertEquals(listOf(NOTES_MISSING), unreadLines(emptyList(), 0, true, false))
    assertEquals(listOf("SMS de Marie : salut", "Il en reste 1.", NOTES_MISSING), unreadLines(h, 1, true, true))
  }

  @Test fun `issue de l'envoi annoncée sans fausse certitude`() {
    assertEquals("Message envoyé.", sentText(Sent.OK))
    assertEquals("Envoi impossible.", sentText(Sent.FAILED))
    assertEquals("Envoi non confirmé. Vérifiez avant de réessayer.", sentText(Sent.UNKNOWN))
    assertEquals("Message envoyé.", noteReplyText(NoteReply.SENT))
    assertEquals("Ce message n'est plus disponible.", noteReplyText(NoteReply.GONE))
    assertEquals("Envoi impossible.", noteReplyText(NoteReply.FAILED))
    assertEquals("Envoi impossible.", noteReplyText(null))
  }

  @Test fun `conversations de groupe reconnues`() {
    assertFalse(groupConversation(false, null, listOf("Léa", "lea", "LÉA")))
    assertFalse(groupConversation(false, " ", listOf("Léa", "")))
    assertTrue(groupConversation(true, null, listOf("Léa")))
    assertTrue(groupConversation(false, "Famille", listOf("Léa")))
    assertTrue(groupConversation(false, null, listOf("Léa", "Paul")))
  }

  @Test fun `réponse refusée si l'expéditeur a changé`() {
    assertTrue(sameSender("Léa", listOf("Paul", "lea")))
    assertFalse(sameSender("Léa", listOf("Paul")))
    assertFalse(sameSender("", listOf("")))
  }

  @Test fun `délai de garde d'une phrase proportionnel à sa longueur`() {
    assertEquals(5_000L, utteranceGuardMs(""))
    assertEquals(5_000L + 120L * 10, utteranceGuardMs("abcdefghij"))
    assertEquals(2 * (5_000L + 120L * 10), utteranceGuardMs("abcdefghij", 0.5f))
    assertEquals((5_000L + 120L * 10) / 2, utteranceGuardMs("abcdefghij", 9f))
  }

  @Test fun `phrase d'échec de la musique`() {
    assertTrue(musicFailure(false).startsWith("Je n'ai rien à piloter."))
    assertEquals("L'application de musique n'a pas répondu.", musicFailure(true))
    assertTrue(musicFailure(true, false).contains("par-dessus les autres applications"))
  }

  @Test fun `notifications d'un contact rapprochées par nom ou numéro`() {
    val marie = Contact("Marie Dupont", listOf("0611223344"))
    assertTrue(fromSender("marie", marie, "Marie Dupont"))
    assertTrue(fromSender("marie", marie, "+33 6 11 22 33 44"))
    assertFalse(fromSender("marie", marie, "Marie Martin"))
    assertTrue(fromSender("marie", null, "Marie Martin"))
    assertFalse(fromSender("marie", null, "Paul"))
    assertFalse(fromSender("", null, "Paul"))
  }

  @Test fun `ordre entendu affiché entre guillemets`() {
    assertEquals("« lis mes messages »", heardText(" lis mes messages "))
  }

  @Test fun `mémoire des messages sans doublon, bornée dans le temps et aux applications cochées`() {
    val day = 86_400_000L
    val now = 100 * day
    val old = listOf(Pending("log#a", "sms.app", "Messages", "Maman", "coucou", now - 2 * day, false), Pending("log#b", "org.signal", "Signal", "Paul", "vieux", now - 40 * day, false))
    val fresh = listOf(Pending("k#1", "sms.app", "Messages", "maman", "coucou", now - 2 * day + 5_000, true), Pending("k#2", "sms.app", "Messages", "Maman", "tu viens ?", now - 1000, true),
      Pending("k#3", "com.autre", "Autre", "X", "y", now, true))
    val all = logMerge(old, fresh, now, setOf("sms.app", "org.signal"))
    assertEquals(listOf("coucou", "tu viens ?"), all.map { it.text })
    assertTrue(all.all { !it.replyable && it.key.startsWith("log#") })
    assertEquals(all, logMerge(all, fresh, now, setOf("sms.app", "org.signal")))
    assertTrue(logMerge(all, emptyList(), now, emptySet()).isEmpty())
  }
}
