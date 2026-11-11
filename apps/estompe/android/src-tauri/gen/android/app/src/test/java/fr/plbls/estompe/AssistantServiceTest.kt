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
    assertEquals(listOf(Heard("", "Marie", "salut"), Heard("Signal", "Léa", "coucou"), Heard("", "Paul", "ok")), items.map { it.heard })
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
    assertEquals("X : un, a ; deux, b ; trois, c ; quatre, d. Lequel ?", choices("X", listOf("a", "b", "c", "d", "e")))
  }

  @Test fun `confirmation, jamais d'envoi sans accord explicite`() {
    assertEquals(Confirm.SEND, confirmStep(parseAnswer("envoyer"), 1))
    assertEquals(Confirm.RETRY, confirmStep(parseAnswer("recommencer"), 1))
    assertEquals(Confirm.CANCEL, confirmStep(parseAnswer("annuler"), 1))
    assertEquals(Confirm.CANCEL, confirmStep(parseAnswer("ne pas envoyer"), 1))
    assertEquals(Confirm.ASK, confirmStep(parseAnswer(""), 1))
    assertEquals(Confirm.ASK, confirmStep(parseAnswer("bof"), 1))
    assertEquals(Confirm.CANCEL, confirmStep(parseAnswer(""), 2))
    assertEquals(Confirm.CANCEL, confirmStep(parseAnswer("hein"), 2))
    for (a in Answer.values()) for (n in 1..3) if (a != Answer.SEND) assertFalse(confirmStep(a, n) == Confirm.SEND)
  }

  @Test fun `relecture du message avant envoi`() {
    assertEquals("SMS pour Marie : j'arrive. Envoyer, annuler ou recommencer ?", review(ReplyTo.Text("06", "Marie"), "j'arrive"))
    assertEquals("Réponse à Léa sur Signal : oui. Envoyer, annuler ou recommencer ?", review(ReplyTo.Note("k", "Léa", "Signal", true), "oui"))
  }

  @Test fun `phrase d'échec de la musique`() {
    assertTrue(musicFailure(false).startsWith("Je n'ai rien à piloter."))
    assertEquals("L'application de musique n'a pas répondu.", musicFailure(true))
  }
}
