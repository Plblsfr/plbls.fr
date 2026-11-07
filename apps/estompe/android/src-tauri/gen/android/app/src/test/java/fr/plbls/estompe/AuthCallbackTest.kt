package fr.plbls.estompe

import org.junit.Assert.assertEquals
import org.junit.Test

class AuthCallbackTest {
  private val code = "fr.plbls.estompe:/callback?state=s&session_state=x&iss=https%3A%2F%2Fauth.plbls.fr&code=abc"
  private val expired = "fr.plbls.estompe:/callback?error=authentication_expired&state=s"

  @Test fun `un retour d'erreur ne remplace pas un retour avec code`() {
    assertEquals(code, keptAuthUrl(code, expired))
  }

  @Test fun `un retour remplace l'absence de retour`() {
    assertEquals(expired, keptAuthUrl("", expired))
    assertEquals(code, keptAuthUrl("", code))
  }

  @Test fun `un nouveau retour avec code remplace le précédent`() {
    val autre = "fr.plbls.estompe:/callback?state=t&code=def"
    assertEquals(autre, keptAuthUrl(code, autre))
    assertEquals(code, keptAuthUrl(expired, code))
  }

  @Test fun `un paramètre dont le nom finit par code n'est pas un code`() {
    assertEquals(expired, keptAuthUrl("fr.plbls.estompe:/callback?error=x&xcode=1", expired))
  }
}
