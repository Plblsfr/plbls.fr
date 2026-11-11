package fr.plbls.estompe

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class AssistantTest {
  private val ytm = MusicApp("yt.music", "YouTube Music")
  private val vlc = MusicApp("org.vlc", "VLC")
  private val apps = listOf(ytm, vlc, MusicApp("com.spotify", "Spotify"))

  private fun cmd(s: String) = parseCommand(s, apps)
  private fun contact(name: String, vararg n: String) = Contact(name, n.toList())

  @Test fun `le repli retire majuscules, accents et ponctuation`() {
    assertEquals("l ete a noel j arrive", fold("  L'été à Noël, j'arrive !  "))
    assertEquals("coeur", fold("Cœur"))
    assertEquals("", fold("?!"))
  }

  @Test fun `le mot d'éveil rend le reste de la phrase`() {
    assertEquals("lis mes messages", wakeWord("estompe lis mes messages"))
    assertEquals("", wakeWord("estompe"))
    assertNull(wakeWord("lis mes messages"))
    assertNull(wakeWord(""))
  }

  @Test fun `le reste suit la dernière occurrence et garde accents et apostrophes`() {
    assertEquals("pause", wakeWord("estompe estompe pause"))
    assertEquals("dis à marie que j'arrive", wakeWord("Estompe, dis à marie que j'arrive"))
  }

  @Test fun `formes proches du mot d'éveil`() {
    assertEquals("pause", wakeWord("estompes pause"))
    assertEquals("pause", wakeWord("estompé pause"))
    assertEquals("pause", wakeWord("estomper pause"))
    assertEquals("pause", wakeWord("l'estompe pause"))
    assertEquals("pause", wakeWord("est tombe pause"))
    assertEquals("pause", wakeWord("et stomp pause"))
  }

  @Test fun `pas de déclenchement sur des mots sans rapport`() {
    assertNull(wakeWord("j'ai mal à l'estomac"))
    assertNull(wakeWord("il est tombé malade"))
    assertNull(wakeWord("et stop la musique"))
    assertNull(wakeWord("le soleil est tombé"))
  }

  @Test fun `la forme en deux mots ne vaut qu'en début de phrase`() {
    assertEquals("malade", wakeWord("est tombé malade"))
    assertEquals(Command.Unknown, parseCommand(wakeWord("est tombé malade")!!))
  }

  @Test fun `lecture des messages non lus`() {
    for (s in listOf("lis mes messages", "lis-moi mes sms", "quels sont mes messages", "est-ce que j'ai des messages",
      "mes nouveaux messages", "lis mes nouveaux messages", "peux-tu me lire mes messages s'il te plaît", "lire les messages non lus")) {
      assertEquals(s, Command.ReadUnread, cmd(s))
    }
  }

  @Test fun `lecture des messages d'un contact`() {
    assertEquals(Command.ReadFrom("marie"), cmd("lis les messages de marie"))
    assertEquals(Command.ReadFrom("jean dupont"), cmd("Lis le dernier message de Jean Dupont"))
    assertEquals(Command.ReadFrom("marie"), cmd("qu'est-ce que m'a écrit marie"))
    assertEquals(Command.ReadFrom("marie"), cmd("lis mes messages de marie"))
    assertEquals(Command.ReadFrom("alice"), cmd("lis-moi les sms d'alice"))
    assertEquals(Command.ReadFrom("marie"), cmd("quels sont les messages de marie"))
  }

  @Test fun `répondre`() {
    for (s in listOf("réponds", "répondre", "réponds-lui", "répondez")) assertEquals(s, Command.Reply, cmd(s))
  }

  @Test fun `envoi d'un SMS avec seulement le destinataire`() {
    assertEquals(Command.SendSms("marie", ""), cmd("envoie un sms à marie"))
    assertEquals(Command.SendSms("marie", ""), cmd("envoie un message à marie"))
    assertEquals(Command.SendSms("marie", ""), cmd("écris à marie"))
    assertEquals(Command.SendSms("jean dupont", ""), cmd("peux-tu envoyer un texto à jean dupont s'il te plaît"))
  }

  @Test fun `envoi d'un SMS avec le texte coupé sur un marqueur`() {
    assertEquals(Command.SendSms("marie", "j'arrive dans dix minutes"), cmd("dis à marie que j'arrive dans dix minutes"))
    assertEquals(Command.SendSms("marie", "j'arrive"), cmd("envoie un sms à marie pour lui dire que j'arrive"))
    assertEquals(Command.SendSms("marie", "je suis en retard"), cmd("envoie un message à marie en disant que je suis en retard"))
    assertEquals(Command.SendSms("marie", "il arrive"), cmd("dis à marie qu'il arrive"))
    assertEquals(Command.SendSms("marie", ""), cmd("dis à marie que"))
  }

  @Test fun `sans marqueur tout ce qui suit à est le nom du contact`() {
    assertEquals(Command.SendSms("marie j arrive", ""), cmd("envoie un sms à marie j'arrive"))
  }

  @Test fun `envoyer sans destinataire ne fait rien`() {
    assertEquals(Command.Unknown, cmd("envoie"))
    assertEquals(Command.Unknown, cmd("envoie un sms"))
    assertEquals(Command.Unknown, cmd("écris"))
  }

  @Test fun `transport de la musique`() {
    val cases = mapOf(
      Transport.PLAY to listOf("lecture", "reprends", "reprends la musique", "play", "mets la musique", "relance la musique", "joue de la musique"),
      Transport.PAUSE to listOf("pause", "stop", "arrête la musique", "coupe la musique", "mets en pause", "arrête", "mets la musique en pause"),
      Transport.NEXT to listOf("suivant", "suivante", "chanson suivante", "passe", "morceau suivant", "passe à la suivante"),
      Transport.PREVIOUS to listOf("précédent", "morceau précédent", "reviens en arrière", "chanson précédente"),
      Transport.VOLUME_UP to listOf("plus fort", "monte le son", "augmente le volume", "mets plus fort"),
      Transport.VOLUME_DOWN to listOf("moins fort", "baisse le son", "baisse le volume", "plus bas"),
    )
    for ((t, list) in cases) for (s in list) assertEquals(s, Command.Music(t, ""), cmd(s))
  }

  @Test fun `priorités entre pause, lecture et recherche`() {
    assertEquals(Command.Music(Transport.PAUSE, ""), cmd("mets en pause"))
    assertEquals(Command.Music(Transport.PLAY, ""), cmd("mets la musique"))
    assertEquals(Command.PlaySearch("daft punk", ""), cmd("mets daft punk"))
    assertEquals(Command.ReadFrom("marie"), cmd("lis mes messages de marie"))
    assertEquals(Command.Music(Transport.VOLUME_UP, ""), cmd("mets le son plus fort"))
  }

  @Test fun `recherche de musique sans les déterminants de tête`() {
    assertEquals(Command.PlaySearch("jazz", ""), cmd("joue du jazz"))
    assertEquals(Command.PlaySearch("discovery", ""), cmd("mets l'album discovery"))
    assertEquals(Command.PlaySearch("daft punk", ""), cmd("je veux écouter daft punk"))
    assertEquals(Command.PlaySearch("get lucky", ""), cmd("mets la chanson get lucky"))
    assertEquals(Command.PlaySearch("rock", ""), cmd("mets un peu de rock"))
    assertEquals(Command.PlaySearch("daft punk", ""), cmd("mets la musique de daft punk"))
    assertEquals(Command.PlaySearch("musique classique", ""), cmd("mets de la musique classique"))
    assertEquals(Command.PlaySearch("daft punk", ""), cmd("est-ce que tu peux mettre daft punk s'il te plaît"))
  }

  @Test fun `playlists`() {
    assertEquals(Command.PlayPlaylist("route", ""), cmd("lance ma playlist route"))
    assertEquals(Command.PlayPlaylist("sport", ""), cmd("mets la playlist sport"))
    assertEquals(Command.PlayPlaylist("soiree", ""), cmd("joue ma liste de lecture soirée"))
    assertEquals(Command.PlayPlaylist("sport", ""), cmd("lance ma playlist de sport"))
    assertEquals(Command.Unknown, cmd("lance ma playlist"))
  }

  @Test fun `application reconnue par un libellé complet ou partiel`() {
    assertEquals(Command.PlaySearch("daft punk", ytm.id), cmd("lance daft punk sur youtube music"))
    assertEquals(Command.PlaySearch("daft punk", ytm.id), cmd("lance daft punk sur youtube"))
    assertEquals(Command.Music(Transport.PAUSE, vlc.id), cmd("pause avec vlc"))
    assertEquals(Command.PlayPlaylist("route", "com.spotify"), cmd("lance ma playlist route sur spotify"))
    assertEquals(Command.Music(Transport.PLAY, ytm.id), cmd("mets la musique sur l'application youtube music"))
  }

  @Test fun `application inconnue ou ambiguë reste dans la requête`() {
    assertEquals(Command.PlaySearch("daft punk sur deezer", ""), cmd("mets daft punk sur deezer"))
    val deux = listOf(MusicApp("a", "YouTube Music"), MusicApp("b", "YouTube Vanced"))
    assertEquals(Command.PlaySearch("daft punk sur youtube", ""), parseCommand("mets daft punk sur youtube", deux))
    assertEquals(Command.Music(Transport.PAUSE, "a"), parseCommand("pause sur youtube music", deux))
    assertEquals(Command.PlaySearch("daft punk sur youtube", ""), parseCommand("mets daft punk sur youtube"))
  }

  @Test fun `annuler et aide`() {
    for (s in listOf("annule", "annuler", "laisse tomber", "rien", "non rien")) assertEquals(s, Command.Cancel, cmd(s))
    for (s in listOf("aide", "qu'est-ce que tu sais faire", "que peux-tu faire")) assertEquals(s, Command.Help, cmd(s))
  }

  @Test fun `phrases ordinaires hors sujet`() {
    for (s in listOf("il fait beau aujourd'hui", "je vais chez marie", "", "   ", "marie", "bonjour tout le monde", "j'ai faim",
      "mets", "lis", "joue au foot", "mets-toi au travail", "quel temps fait-il", "ma playlist", "on se voit demain à la plage", "pause café")) {
      assertEquals(s, Command.Unknown, cmd(s))
    }
  }

  @Test fun `un mot de remplissage seul ne déclenche rien`() {
    assertEquals(Command.Unknown, cmd("s'il te plaît"))
    assertEquals(Command.Unknown, cmd("est-ce que tu peux"))
  }

  @Test fun `les mots de politesse en tête et en queue sont ignorés`() {
    assertEquals(Command.Music(Transport.NEXT, ""), cmd("euh suivant s'il vous plaît"))
    assertEquals(Command.ReadUnread, cmd("pourriez-vous lire mes messages merci"))
  }

  private val book = listOf(
    contact("Marie Dupont", "0611111111"), contact("Marie Martin", "0622222222"), contact("Jean Dupont", "0633333333"),
    contact("Éloïse Durand", "0644444444"), contact("Sans Numéro"), contact("Paul", "0655555555"),
  )

  @Test fun `contact trouvé par son nom complet`() {
    assertEquals(listOf("Marie Dupont"), matchContacts("marie dupont", book).map { it.name })
    assertEquals(listOf("Éloïse Durand"), matchContacts("Eloise Durand", book).map { it.name })
  }

  @Test fun `un prénom seul renvoie tous les contacts de ce prénom`() {
    assertEquals(listOf("Marie Dupont", "Marie Martin"), matchContacts("marie", book).map { it.name })
    assertEquals(listOf("Marie Dupont", "Jean Dupont"), matchContacts("dupont", book).map { it.name })
    assertEquals(listOf("Marie Martin"), matchContacts("martin marie", book).map { it.name })
  }

  @Test fun `erreurs de transcription absorbées par la proximité`() {
    assertEquals(listOf("Marie Dupont", "Marie Martin"), matchContacts("mari", book).map { it.name })
    assertEquals(listOf("Jean Dupont"), matchContacts("jean dupond", book).map { it.name })
    assertEquals(listOf("Éloïse Durand"), matchContacts("eloise durant", book).map { it.name })
  }

  @Test fun `jamais de candidat lointain`() {
    assertTrue(matchContacts("robert", book).isEmpty())
    assertTrue(matchContacts("paulette martinez", book).isEmpty())
    assertTrue(matchContacts("", book).isEmpty())
    assertTrue(matchContacts("pol", book).isEmpty())
  }

  @Test fun `les contacts sans numéro sont ignorés`() {
    assertTrue(matchContacts("sans numero", book).isEmpty())
    assertTrue(matchContacts("marie", listOf(Contact("Marie", listOf("  ")))).isEmpty())
  }

  @Test fun `mêmes numéros malgré les formats`() {
    assertTrue(samePhone("+33 6 12 34 56 78", "0612345678"))
    assertTrue(samePhone("06.12.34.56.78", "+33612345678"))
    assertTrue(samePhone("0033612345678", "06 12 34 56 78"))
    assertTrue(samePhone("+33 (0)6 12 34 56 78", "0612345678"))
  }

  @Test fun `numéros différents, courts ou alphanumériques`() {
    assertFalse(samePhone("0612345678", "0612345679"))
    assertFalse(samePhone("", ""))
    assertTrue(samePhone("36 31", "3631"))
    assertFalse(samePhone("3631", "3632"))
    assertTrue(samePhone("Banque", "Banque"))
    assertFalse(samePhone("Banque", "0612345678"))
    assertFalse(samePhone("Banque", "Poste"))
  }

  @Test fun `choix parmi une liste énoncée`() {
    assertEquals(0, pickChoice("le premier", 3))
    assertEquals(0, pickChoice("premier", 3))
    assertEquals(0, pickChoice("un", 3))
    assertEquals(1, pickChoice("numéro deux", 3))
    assertEquals(1, pickChoice("la deuxième", 3))
    assertEquals(2, pickChoice("trois", 3))
    assertEquals(2, pickChoice("le dernier", 3))
    assertEquals(1, pickChoice("la dernière", 2))
  }

  @Test fun `choix hors liste ou incompréhensible`() {
    assertNull(pickChoice("le quatrième", 3))
    assertNull(pickChoice("le dernier", 0))
    assertNull(pickChoice("annule", 3))
    assertNull(pickChoice("", 3))
    assertNull(pickChoice("je ne sais pas trop quoi choisir parmi tout ça un jour", 3))
  }

  @Test fun `annonce regroupée par expéditeur consécutif`() {
    val items = listOf(Heard("", "Marie", "premier texte"), Heard("", "Marie", "Deuxième texte"), Heard("", "Paul", "salut"))
    assertEquals(listOf("2 messages de Marie : premier texte. Deuxième texte.", "Message de Paul : salut"), announceMessages(items))
  }

  @Test fun `annonce avec la source quand ce n'est pas un SMS`() {
    assertEquals(listOf("Message de Jean sur Signal : texte"), announceMessages(listOf(Heard("Signal", "Jean", "texte"))))
  }

  @Test fun `expéditeurs et sources différents ne sont pas fusionnés`() {
    val items = listOf(Heard("", "Marie", "a"), Heard("Signal", "Marie", "b"), Heard("", "Marie", "c"), Heard("", "Marie", "d"))
    assertEquals(listOf("Message de Marie : a", "Message de Marie sur Signal : b", "2 messages de Marie : c. d."), announceMessages(items))
  }

  @Test fun `élision devant une voyelle`() {
    assertEquals(listOf("Message d'Alice : coucou"), announceMessages(listOf(Heard("", "Alice", "coucou"))))
    assertEquals(listOf("2 messages d'Éloïse : a. b."), announceMessages(listOf(Heard("", "Éloïse", "a"), Heard("", "Éloïse", "b"))))
  }

  @Test fun `texte nettoyé, ponctuation conservée`() {
    val items = listOf(Heard("", "Marie", "regarde https://exemple.fr/x 😀"), Heard("", "Marie", "ça va ?"))
    assertEquals(listOf("2 messages de Marie : regarde un lien. ça va ?"), announceMessages(items))
  }

  @Test fun `message sans texte`() {
    assertEquals(listOf("Marie a envoyé un message sans texte"), announceMessages(listOf(Heard("", "Marie", "😀"))))
    assertEquals(listOf("Marie a envoyé 2 messages sans texte sur Signal"), announceMessages(listOf(Heard("Signal", "Marie", ""), Heard("Signal", "Marie", "  "))))
  }

  @Test fun `liste vide`() {
    assertEquals(listOf("Aucun nouveau message."), announceMessages(emptyList()))
  }
}
