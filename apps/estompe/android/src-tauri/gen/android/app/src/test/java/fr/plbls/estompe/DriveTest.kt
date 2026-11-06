package fr.plbls.estompe

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class DriveTest {
  private fun data(vararg kv: Pair<String, String>) = mapOf("tag" to "msg-1", "kind" to "dm", "conv" to "d:9", "msg" to "1",
    "sender" to "alice", "text" to "salut", "title" to "alice") + kv

  private fun msg(vararg kv: Pair<String, String>) = Incoming.from(data(*kv))!!

  @Test fun `annonce d'un message privé et d'une mention de salon`() {
    assertEquals("Message de alice : salut", announce(msg(), true))
    assertEquals("Message de alice dans général : salut", announce(msg("room" to "général"), true))
  }

  @Test fun `l'annonce peut se limiter à l'expéditeur`() {
    assertEquals("Message de alice dans général", announce(msg("room" to "général"), false))
  }

  @Test fun `pièces jointes, liens et smileys sont résumés`() {
    assertEquals("Message de alice : a envoyé une image", announce(msg("text" to "", "attach" to "image"), true))
    assertEquals("Message de alice : regarde. a envoyé un fichier", announce(msg("text" to "regarde", "attach" to "file"), true))
    assertEquals("Message de alice : a envoyé un smiley", announce(msg("text" to "😀"), true))
    assertEquals("Message de alice : va sur un lien", announce(msg("text" to "va sur https://exemple.fr/x?y=1"), true))
    assertEquals("bonjour", spoken("**bonjour** :tada: 👋"))
  }

  @Test fun `aperçu masqué, seul le titre est lu`() {
    val m = msg("sender" to "", "text" to "", "title" to "Nouveau message de alice")
    assertTrue(m.hidden)
    assertEquals("Nouveau message de alice", announce(m, true))
  }

  @Test fun `seuls les messages privés et mentions sont lus`() {
    assertTrue(msg("kind" to "mention").kind in Incoming.SPOKEN_KINDS)
    assertFalse(msg("kind" to "channel").kind in Incoming.SPOKEN_KINDS)
    assertFalse(msg("kind" to "voice").kind in Incoming.SPOKEN_KINDS)
    assertNull(Incoming.from(mapOf("kind" to "dm")))
  }

  @Test fun `réponse possible seulement dans une conversation`() {
    assertTrue(msg().replyable)
    assertFalse(msg("conv" to "", "msg" to "").replyable)
  }

  @Test fun `la file lit dans l'ordre d'arrivée et ignore les doublons`() {
    val q = ReadQueue()
    assertTrue(q.offer(msg("tag" to "a")))
    assertTrue(q.offer(msg("tag" to "b")))
    assertFalse(q.offer(msg("tag" to "a")))
    assertEquals(2, q.size)
    assertEquals("a", q.poll()?.tag)
    assertEquals("b", q.poll()?.tag)
    assertNull(q.poll())
    assertFalse(q.offer(msg("tag" to "a"))) // déjà lu
  }

  @Test fun `réponse parlée de confirmation`() {
    assertEquals(Answer.SEND, parseAnswer("envoyer"))
    assertEquals(Answer.SEND, parseAnswer("Oui"))
    assertEquals(Answer.CANCEL, parseAnswer("annuler"))
    assertEquals(Answer.CANCEL, parseAnswer("ne pas envoyer"))
    assertEquals(Answer.RETRY, parseAnswer("recommencer"))
    assertEquals(Answer.RETRY, parseAnswer("répéter"))
    assertEquals(Answer.UNKNOWN, parseAnswer("bof"))
    assertEquals(Answer.UNKNOWN, parseAnswer(""))
  }

  @Test fun `la vitesse de voix est bornée`() {
    assertEquals(2f, DriveSettings(rate = 9f).safeRate, 0f)
    assertEquals(0.5f, DriveSettings(rate = 0f).safeRate, 0f)
    assertEquals(1f, DriveSettings().safeRate, 0f)
  }
}
