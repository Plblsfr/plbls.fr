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
    assertEquals("pause", wakeWord("estomp pause"))
    assertEquals("pause", wakeWord("l'estompe pause"))
    assertEquals("pause", wakeWord("est tombe pause"))
    assertEquals("pause", wakeWord("et stomp pause"))
  }

  @Test fun `pas de déclenchement sur des mots sans rapport`() {
    assertNull(wakeWord("j'ai mal à l'estomac"))
    assertNull(wakeWord("il est tombé malade"))
    assertNull(wakeWord("et stop la musique"))
    assertNull(wakeWord("le soleil est tombé"))
    for (s in listOf("il faut estomper la couleur", "estompez les bords", "ils estompent le bruit")) assertNull(s, wakeWord(s))
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
    assertEquals(Command.ReadFrom("Jean Dupont", true), cmd("Lis le dernier message de Jean Dupont"))
    assertEquals(Command.ReadFrom("marie"), cmd("qu'est-ce que m'a écrit marie"))
    assertEquals(Command.ReadFrom("marie"), cmd("lis mes messages de marie"))
    assertEquals(Command.ReadFrom("alice"), cmd("lis-moi les sms d'alice"))
    assertEquals(Command.ReadFrom("Éloïse Durand"), cmd("Lis les messages d'Éloïse Durand"))
    assertEquals(Command.ReadFrom("marie"), cmd("lis les messages de marie sur whatsapp"))
    assertEquals(Command.ReadFrom("Marie Dupont"), cmd("lis les messages de Marie Dupont sur signal"))
    assertEquals(Command.ReadFrom("marie"), cmd("quels sont les messages de marie"))
  }

  @Test fun `déclaratifs et ambigus ne lisent pas les messages`() {
    for (s in listOf("j'ai reçu un message", "j'ai un message", "il y a un message", "j'ai des messages", "j'ai des messages ?")) assertEquals(s, Command.Unknown, cmd(s))
    for (s in listOf("ai-je des messages", "y a-t-il des messages", "est-ce qu'il y a des messages", "quels sont mes messages", "ai-je reçu un message")) assertEquals(s, Command.ReadUnread, cmd(s))
  }

  @Test fun `répondre`() {
    for (s in listOf("réponds", "répondre", "réponds-lui", "répondez")) assertEquals(s, Command.Reply, cmd(s))
  }

  @Test fun `envoi d'un SMS avec seulement le destinataire`() {
    assertEquals(Command.SendSms("marie", ""), cmd("envoie un sms à marie"))
    assertEquals(Command.SendSms("marie", ""), cmd("envoie un message à marie"))
    assertEquals(Command.SendSms("marie", ""), cmd("écris à marie"))
    assertEquals(Command.SendSms("jean dupont", ""), cmd("peux-tu envoyer un texto à jean dupont s'il te plaît"))
    assertEquals(Command.SendSms("Éloïse Durand", ""), cmd("Envoie un SMS à Éloïse Durand"))
    assertEquals(Command.SendSms("mère demain", ""), cmd("envoie un sms à ma mère demain"))
    assertEquals(Command.SendSms("Marie", "J'arrive"), cmd("Dis à Marie que J'arrive"))
  }

  @Test fun `envoi d'un SMS avec le texte coupé sur un marqueur`() {
    assertEquals(Command.SendSms("marie", "j'arrive dans dix minutes"), cmd("dis à marie que j'arrive dans dix minutes"))
    assertEquals(Command.SendSms("marie", "j'arrive"), cmd("envoie un sms à marie pour lui dire que j'arrive"))
    assertEquals(Command.SendSms("marie", "je suis en retard"), cmd("envoie un message à marie en disant que je suis en retard"))
    assertEquals(Command.SendSms("marie", "il arrive"), cmd("dis à marie qu'il arrive"))
    assertEquals(Command.SendSms("marie", ""), cmd("dis à marie que"))
  }

  @Test fun `sans marqueur tout ce qui suit à est le nom du contact, texte original`() {
    assertEquals(Command.SendSms("marie j'arrive", ""), cmd("envoie un sms à marie j'arrive"))
    assertEquals(Command.SendSms("Marie j'arrive", ""), cmd("Envoie un SMS à Marie j'arrive."))
  }

  @Test fun `séparer destinataire et texte`() {
    val b = listOf(contact("Marie Dupont", "0611111111"), contact("Marie Martin", "0622222222"), contact("Jean Dupont", "0633333333"), contact("Paul", "0655555555"))
    val (m, t) = splitRecipient("marie j'arrive dans dix minutes", b)
    assertEquals(listOf("Marie Dupont", "Marie Martin"), m.map { it.name })
    assertEquals("j'arrive dans dix minutes", t)
    val (j, t2) = splitRecipient("jean dupont je suis en retard", b)
    assertEquals(listOf("Jean Dupont"), j.map { it.name })
    assertEquals("je suis en retard", t2)
    val (w, t3) = splitRecipient("Jean Dupont", b)
    assertEquals(listOf("Jean Dupont"), w.map { it.name })
    assertEquals("", t3)
    assertEquals(emptyList<Contact>() to "", splitRecipient("ma mère demain", b))
    assertEquals(emptyList<Contact>() to "", splitRecipient("robert bonjour à tous", b))
    assertEquals(emptyList<Contact>() to "", splitRecipient("", b))
    val (p, t4) = splitRecipient("Paul Demain Matin", b)
    assertEquals(listOf("Paul"), p.map { it.name })
    assertEquals("Demain Matin", t4)
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
    assertEquals(Command.Music(Transport.PAUSE, ""), cmd("mets le sur pause"))
  }

  @Test fun `volume à fond ou plus bas`() {
    for (s in listOf("mets le volume à fond", "mets le son à fond", "monte le volume au maximum")) assertEquals(s, Command.Music(Transport.VOLUME_UP, ""), cmd(s))
    for (s in listOf("mets le volume plus bas", "baisse un peu", "baisse")) assertEquals(s, Command.Music(Transport.VOLUME_DOWN, ""), cmd(s))
  }

  @Test fun `une recherche musicale ne s'ouvre pas sur un objet du quotidien`() {
    for (s in listOf("mets la lumière", "mets le feu", "mets le couvert", "mets la télé", "mets un minuteur", "mets le réveil à 7 heures", "mets stop", "écoute bien",
      "écoute-moi bien", "écoute moi", "lance moi", "mets ça", "mets-le", "mets la", "lance l'application", "lance la machine à laver", "écoute ce que Marie m'a dit",
      "mets les messages de Marie", "mets mes messages", "mets ton manteau", "lance un minuteur", "mets le chauffage", "mets le volume", "écoute", "mets ça en route")) {
      assertEquals(s, Command.Unknown, cmd(s))
    }
  }

  @Test fun `titres commençant par une préposition`() {
    assertEquals(Command.PlaySearch("sur la route de memphis", ""), cmd("mets sur la route de memphis"))
    assertEquals(Command.PlaySearch("avec le temps", ""), cmd("mets avec le temps"))
    assertEquals(Command.PlaySearch("sur mesure", ""), cmd("mets sur mesure"))
    assertEquals(Command.Unknown, cmd("mets sur stop"))
    assertEquals(Command.Unknown, cmd("joue dans le silence"))
  }

  @Test fun `recherches musicales qui continuent de marcher`() {
    assertEquals(Command.PlaySearch("daft punk", ""), cmd("mets daft punk"))
    assertEquals(Command.PlaySearch("jazz", ""), cmd("mets du jazz"))
    assertEquals(Command.PlaySearch("bon jovi", ""), cmd("mets bon jovi"))
    assertEquals(Command.PlaySearch("alors on danse", ""), cmd("mets alors on danse"))
    assertEquals(Command.PlaySearch("nina simone", ""), cmd("écoute nina simone"))
    assertEquals(Command.PlaySearch("daft punk", ""), cmd("lance daft punk"))
    assertEquals(Command.Music(Transport.PLAY, ""), cmd("écoute de la musique"))
  }

  @Test fun `entrées très longues sans débordement`() {
    val long = "euh ".repeat(5000)
    assertEquals(Command.Unknown, parseCommand(long))
    assertNull(wakeWord(long))
    assertEquals(Answer.UNKNOWN, confirmAnswer(long))
    assertNull(pickChoice(long, 3))
    assertEquals("pause", wakeWord("estompe pause" + " ".repeat(5000)))
  }

  @Test fun `confirmation stricte avant l'envoi`() {
    for (s in listOf("n'envoie pas", "ne l'envoie pas", "ne l'envoie surtout pas", "n'envoyez pas", "je l'envoie pas", "pas envoyer", "valide pas", "n'envoie rien",
      "j'ai pas dit envoie", "ok je vais l'envoyer plus tard", "non", "annule", "stop", "surtout pas", "jamais envoie", "attends envoie", "laisse tomber", "tant pis", "oublie")) {
      assertEquals(s, Answer.CANCEL, confirmAnswer(s))
    }
    for (s in listOf("envoyer", "envoie", "oui envoie", "ok envoyer", "vas-y envoie", "Envoie !", "d'accord envoie", "tu peux l'envoyer", "envoie merci")) {
      assertEquals(s, Answer.SEND, confirmAnswer(s))
    }
    for (s in listOf("oui", "ok", "il a dit oui à sa mère", "envoie la photo à paul demain", "valide", "", "   ", "bonjour", "oui oui oui oui envoie", "je vais envoyer un message")) {
      assertEquals(s, Answer.UNKNOWN, confirmAnswer(s))
    }
    for (s in listOf("recommence", "refais", "répète", "redis", "corrige")) assertEquals(s, Answer.RETRY, confirmAnswer(s))
    assertEquals(Answer.CANCEL, confirmAnswer("recommence ne pas envoyer"))
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
    for (s in listOf("annule", "annuler", "laisse tomber", "rien", "non rien", "non annule", "annuler tout", "oublie ça", "laisse tomber", "je ne veux plus", "pas du tout", "non", "tant pis")) assertEquals(s, Command.Cancel, cmd(s))
    assertEquals(Command.Music(Transport.PAUSE, ""), cmd("arrête la musique"))
    assertEquals(Command.Music(Transport.PAUSE, ""), cmd("stop"))
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
    for (s in listOf("il y a un souci", "attends une seconde", "pas le deuxième", "non le premier", "ne prends pas le un", "le premier ou le deuxième s'il te plaît")) assertNull(s, pickChoice(s, 3))
    assertEquals(0, pickChoice("le un", 3))
    assertEquals(0, pickChoice("numéro un", 3))
    assertEquals(0, pickChoice("une", 3))
    assertEquals(1, pickChoice("prends le deuxième", 3))
  }

  @Test fun `annonce regroupée par expéditeur consécutif`() {
    val items = listOf(Heard("", "Marie", "premier texte"), Heard("", "Marie", "Deuxième texte"), Heard("", "Paul", "salut"))
    assertEquals(listOf("2 SMS de Marie : premier texte. Deuxième texte.", "SMS de Paul : salut"), announceMessages(items))
  }

  @Test fun `annonce avec la source quand ce n'est pas un SMS`() {
    assertEquals(listOf("Message de Jean sur Signal : texte"), announceMessages(listOf(Heard("Signal", "Jean", "texte"))))
  }

  @Test fun `expéditeurs et sources différents ne sont pas fusionnés`() {
    val items = listOf(Heard("", "Marie", "a"), Heard("Signal", "Marie", "b"), Heard("", "Marie", "c"), Heard("", "Marie", "d"))
    assertEquals(listOf("SMS de Marie : a", "Message de Marie sur Signal : b", "2 SMS de Marie : c. d."), announceMessages(items))
  }

  @Test fun `élision devant une voyelle`() {
    assertEquals(listOf("SMS d'Alice : coucou"), announceMessages(listOf(Heard("", "Alice", "coucou"))))
    assertEquals(listOf("2 SMS d'Éloïse : a. b."), announceMessages(listOf(Heard("", "Éloïse", "a"), Heard("", "Éloïse", "b"))))
  }

  @Test fun `texte nettoyé, ponctuation conservée`() {
    val items = listOf(Heard("", "Marie", "regarde https://exemple.fr/x 😀"), Heard("", "Marie", "ça va ?"))
    assertEquals(listOf("2 SMS de Marie : regarde un lien. ça va ?"), announceMessages(items))
  }

  @Test fun `message sans texte`() {
    assertEquals(listOf("Marie a envoyé un SMS sans texte"), announceMessages(listOf(Heard("", "Marie", "😀"))))
    assertEquals(listOf("Marie a envoyé 2 messages sans texte sur Signal"), announceMessages(listOf(Heard("Signal", "Marie", ""), Heard("Signal", "Marie", "  "))))
  }

  @Test fun `liste vide`() {
    assertEquals(listOf("Aucun nouveau message."), announceMessages(emptyList()))
  }

  @Test fun `le mot d'éveil mal transcrit est reconnu en tête de phrase`() {
    assertEquals("lis mes messages", wakeWord("est tombée lis mes messages"))
    assertEquals("pause", wakeWord("et tombe pause"))
    assertNull(wakeWord("il est tombé hier"))
  }

  @Test fun `un seul message quand la demande est au singulier`() {
    assertEquals(Command.ReadFrom("marie", true), cmd("lis le message de marie"))
    assertEquals(Command.ReadFrom("marie", true), cmd("lis-moi le dernier sms de marie"))
    assertEquals(Command.ReadFrom("marie", true), cmd("le dernier message de marie"))
    assertEquals(Command.ReadFrom("marie", false), cmd("lis les derniers messages de marie"))
    assertEquals(Command.ReadFrom("marie", false), cmd("lis les sms de marie"))
  }

  @Test fun `ouvrir une application de musique par son nom, même mal transcrit`() {
    for (s in listOf("lance youtube music", "lance youtube musique", "ouvre you tube musique", "mets youtube", "démarre l'application youtube music", "lance yt music",
      "est-ce que tu peux lancer youtube musique s'il te plaît")) {
      assertEquals(s, Command.Music(Transport.PLAY, ytm.id), cmd(s))
    }
    assertEquals(Command.Music(Transport.PLAY, "com.spotify"), cmd("ouvre spotify"))
    assertEquals(Command.Music(Transport.PLAY, "yt"), parseCommand("lance youtube musique", listOf(MusicApp("yt", "YT Music"), vlc)))
    assertEquals(Command.PlaySearch("daft punk", ytm.id), cmd("mets daft punk sur youtube musique"))
    assertEquals(Command.PlaySearch("deezer", ""), cmd("lance deezer"))
    assertEquals(Command.Unknown, cmd("ouvre la fenêtre"))
  }

  @Test fun `date d'un message dite simplement`() {
    val utc = java.util.TimeZone.getTimeZone("UTC")
    val now = 1_763_042_400_000L
    assertEquals("", whenText(0, now, utc))
    assertEquals("à l'instant", whenText(now - 30_000, now, utc))
    assertEquals("à l'instant", whenText(now + 60_000, now, utc))
    assertEquals("il y a 12 minutes", whenText(now - 12 * 60_000, now, utc))
    assertEquals("à 9 h 05", whenText(now - 4 * 3_600_000 - 55 * 60_000, now, utc))
    assertEquals("hier à 14 h", whenText(now - 24 * 3_600_000, now, utc))
    assertEquals("mardi à 14 h", whenText(now - 2 * 24 * 3_600_000, now, utc))
    assertEquals("le 1er novembre", whenText(now - 12L * 24 * 3_600_000, now, utc))
    assertEquals("le 13 novembre 2024", whenText(now - 365L * 24 * 3_600_000, now, utc))
  }

  @Test fun `l'annonce dit la provenance et la date`() {
    val utc = java.util.TimeZone.getTimeZone("UTC")
    val now = 1_763_042_400_000L
    val items = listOf(Heard("", "Marie", "a", now - 24 * 3_600_000), Heard("", "Marie", "b", now - 12 * 60_000), Heard("WhatsApp", "Paul", "salut", now - 30_000))
    assertEquals(listOf("2 SMS de Marie, le dernier il y a 12 minutes : a. b.", "Message de Paul sur WhatsApp, à l'instant : salut"), announceMessages(items, now, utc))
    assertEquals(listOf("SMS de Marie, hier à 14 h : a"), announceMessages(items.take(1), now, utc))
  }

  @Test fun `chaque ordre de la grammaire est compris par l'analyse`() {
    val phrases = commandPhrases(listOf("Marie Dupont", "Alice", "Jean-Luc", "  ", "Dr. Paul (bureau) 2"), listOf("YouTube Music", "Spotify", "VLC"))
    for (p in phrases) assertTrue(p, parseCommand(p, apps) != Command.Unknown)
    assertTrue(phrases.containsAll(FIXED_ORDERS))
    assertEquals(Command.ReadFrom("marie dupont"), cmd("lis les messages de marie dupont"))
    assertTrue("lis les messages de marie" in phrases)
    assertTrue("lis le dernier message d'alice" in phrases)
    assertTrue("envoie un sms à jean-luc" in phrases)
    assertTrue("lis les messages de dr paul bureau" in phrases)
    assertEquals(Command.Music(Transport.PLAY, ytm.id), cmd("lance youtube music"))
    assertTrue("ouvre youtube music" in phrases)
  }

  @Test fun `la grammaire reste bornée et bien formée`() {
    val many = (0 until 2000).map { n -> "ab" + n.toString(26).map { c -> 'a' + Character.digit(c, 26) }.joinToString("") }
    assertTrue(commandPhrases(many, emptyList()).size < 1500)
    assertEquals("[\"pause\",\"dis \\\"oui\\\"\",\"[unk]\"]", grammarJson(listOf("pause", "dis \"oui\"")))
    assertTrue(wakeGrammar().startsWith("[\"estompe\","))
    assertEquals("jean-luc d'arc pro", grammarWords("  Jean-Luc  D’Arc (pro) 06 "))
  }

  @Test fun `réponses de confirmation et de choix de la grammaire`() {
    for (p in CONFIRM_PHRASES) assertTrue(p, confirmAnswer(p) != Answer.UNKNOWN)
    for (p in CHOICE_PHRASES) assertTrue(p, pickChoice(p, 4) != null || parseCommand(p) == Command.Cancel)
  }

  @Test fun `homophones du premier mot corrigés quand rien n'est compris`() {
    assertEquals(Command.PlaySearch("daft punk", ""), cmd("mais daft punk"))
    assertEquals(Command.Music(Transport.PLAY, ytm.id), cmd("l'ancien youtube music"))
    assertEquals(Command.ReadFrom("marie"), cmd("ii les messages de marie"))
    assertEquals(Command.ReadFrom("paul", true), cmd("lit le dernier message de paul"))
    assertEquals(Command.Music(Transport.PAUSE, ""), cmd("posait"))
    assertEquals(Command.Music(Transport.PLAY, ""), cmd("reprend la musique"))
    assertEquals(Command.PlaySearch("jazz", ""), cmd("jour du jazz"))
    assertEquals(Command.ReadUnread, cmd("mes messages"))
    assertEquals(Command.PlaySearch("allo la peur", ""), cmd("mais allo la peur"))
    assertEquals(Command.Unknown, cmd("mais non"))
    assertEquals(Command.Unknown, cmd("mais oui c'est ça"))
  }

  @Test fun `le mot SMS mal transcrit est rétabli`() {
    assertEquals(Command.ReadFrom("maman", true), cmd("le dernier essaie messe de maman"))
    assertEquals(Command.ReadFrom("maman", true), cmd("lis le dernier essaie messe de maman"))
    assertEquals(Command.ReadFrom("maman"), cmd("messe de maman"))
    assertEquals(Command.ReadFrom("maman", true), cmd("dernier sms de maman"))
    assertEquals(Command.SendSms("marie", ""), cmd("envoie un ses mais à marie"))
    assertEquals(Command.SendSms("marie", ""), cmd("envoie un cms à marie"))
    assertEquals(Command.ReadUnread, cmd("lis mes essaie messe"))
    assertEquals(Command.ReadFrom("maman", true), cmd("lis le dernier sms de maman"))
  }
}
