package fr.plbls.estompe

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import org.json.JSONException
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL
import java.net.URLEncoder
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * Session de l'app côté natif : le refresh token (qui tourne à chaque usage) vit ici, chiffré par une clé AES du Keystore Android,
 * et ne sort jamais vers le front ; celui-ci (via le canal EstompeSession) et les actions de notification obtiennent un jeton d'accès.
 * L'émetteur Keycloak et l'API sont figés au build (BuildConfig, lus dans web/estompe/.env.android).
 */
class Session(context: Context) {
  private val prefs = context.applicationContext.getSharedPreferences("session", Context.MODE_PRIVATE)

  init {
    if (prefs.contains("issuer") || prefs.contains("api")) prefs.edit().remove("issuer").remove("api").apply()
  }

  var refreshToken: String?
    get() = synchronized(LOCK) { lire() }
    set(v) = synchronized(LOCK) { ecrire(v) }

  var fcmToken: String?
    get() = prefs.getString("fcm", null)
    set(v) { prefs.edit().putString("fcm", v).apply() }

  val apiUrl: String get() = BuildConfig.API_URL

  private fun lire(): String? {
    prefs.getString(CLAIR, null)?.let { t ->
      chiffrer(t)?.let { prefs.edit().remove(CLAIR).putString(CHIFFRE, it).apply() }
      return t
    }
    val s = prefs.getString(CHIFFRE, null) ?: return null
    return dechiffrer(s).also { if (it == null) prefs.edit().remove(CHIFFRE).apply() }
  }

  private fun ecrire(v: String?) {
    val e = prefs.edit().remove(CLAIR).remove(CHIFFRE)
    v?.let { chiffrer(it) }?.let { e.putString(CHIFFRE, it) }
    e.apply()
  }

  private fun cle(): SecretKey {
    val ks = KeyStore.getInstance(KEYSTORE).apply { load(null) }
    (ks.getKey(ALIAS, null) as? SecretKey)?.let { return it }
    val g = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, KEYSTORE)
    g.init(KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
      .setBlockModes(KeyProperties.BLOCK_MODE_GCM).setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE).setKeySize(256).build())
    return g.generateKey()
  }

  private fun chiffrer(t: String): String? = try {
    val c = Cipher.getInstance(AES_GCM).apply { init(Cipher.ENCRYPT_MODE, cle()) }
    Base64.encodeToString(c.iv + c.doFinal(t.toByteArray(Charsets.UTF_8)), Base64.NO_WRAP)
  } catch (e: Exception) {
    null
  }

  private fun dechiffrer(s: String): String? = try {
    val b = Base64.decode(s, Base64.NO_WRAP)
    val c = Cipher.getInstance(AES_GCM).apply { init(Cipher.DECRYPT_MODE, cle(), GCMParameterSpec(128, b, 0, IV)) }
    String(c.doFinal(b, IV, b.size - IV), Charsets.UTF_8)
  } catch (e: Exception) {
    null
  }

  private fun form(vararg kv: Pair<String, String>) = kv.joinToString("&") { "${it.first}=${URLEncoder.encode(it.second, "UTF-8")}" }

  private fun poster(chemin: String, corps: String): Pair<Int, JSONObject> {
    val c = URL(BuildConfig.KEYCLOAK_ISSUER + chemin).openConnection() as HttpURLConnection
    try {
      c.requestMethod = "POST"
      c.doOutput = true
      c.connectTimeout = 10_000
      c.readTimeout = 10_000
      c.setRequestProperty("Content-Type", "application/x-www-form-urlencoded")
      c.outputStream.use { it.write(corps.toByteArray()) }
      val code = c.responseCode
      val texte = (if (code in 200..299) c.inputStream else c.errorStream)?.bufferedReader()?.use { it.readText() }.orEmpty()
      return code to (try { JSONObject(texte) } catch (e: JSONException) { JSONObject() })
    } finally {
      c.disconnect()
    }
  }

  private fun jetons(vararg kv: Pair<String, String>): JSONObject {
    if (BuildConfig.KEYCLOAK_ISSUER.isEmpty()) return JSONObject().put("status", 0).put("error", "émetteur Keycloak non configuré")
    val (code, j) = try {
      poster("/protocol/openid-connect/token", form("client_id" to CLIENT_ID, *kv))
    } catch (e: Exception) {
      return JSONObject().put("status", 0).put("error", e.message ?: "réseau indisponible")
    }
    val r = JSONObject().put("status", code)
    if (code == 200) {
      j.optString("refresh_token").takeIf { it.isNotEmpty() }?.let { refreshToken = it }
      r.put("access_token", j.optString("access_token")).put("expires_in", j.optInt("expires_in"))
    } else {
      r.put("error", j.optString("error_description").ifEmpty { j.optString("error") })
    }
    return r
  }

  fun connecter(code: String, verifier: String, redirectUri: String): JSONObject {
    return synchronized(LOCK) { jetons("grant_type" to "authorization_code", "code" to code, "code_verifier" to verifier, "redirect_uri" to redirectUri) }
  }

  /** Jeton d'accès frais (un seul renouvellement à la fois : le refresh token tourne) ; status 401 sans session ou session refusée par Keycloak. */
  fun renouveler(): JSONObject {
    return synchronized(LOCK) {
      val rt = refreshToken ?: return JSONObject().put("status", 401)
      jetons("grant_type" to "refresh_token", "refresh_token" to rt).also { if (it.optInt("status") in setOf(400, 401)) refreshToken = null }
    }
  }

  fun adopter(rt: String) {
    synchronized(LOCK) { if (refreshToken == null && rt.isNotEmpty()) ecrire(rt) }
  }

  fun deconnecter() {
    val rt = synchronized(LOCK) { refreshToken.also { refreshToken = null } } ?: return
    if (BuildConfig.KEYCLOAK_ISSUER.isEmpty()) return
    try { poster("/protocol/openid-connect/logout", form("client_id" to CLIENT_ID, "refresh_token" to rt)) } catch (e: Exception) { }
  }

  private fun accessToken(): String? = renouveler().takeIf { it.optInt("status") == 200 }?.optString("access_token")?.takeIf { it.isNotEmpty() }

  /** Appel authentifié de l'API ; renvoie le code HTTP, ou 0 sans session ou en cas d'échec réseau. */
  fun call(method: String, path: String, body: JSONObject = JSONObject()): Int {
    val api = apiUrl.takeIf { it.isNotEmpty() } ?: return 0
    return try {
      val token = accessToken() ?: return 0
      val c = URL(api + path).openConnection() as HttpURLConnection
      try {
        c.requestMethod = method
        c.connectTimeout = 10_000
        c.readTimeout = 10_000
        c.setRequestProperty("Authorization", "Bearer $token")
        c.setRequestProperty("Content-Type", "application/json")
        c.doOutput = true
        c.outputStream.use { it.write(body.toString().toByteArray()) }
        c.responseCode
      } finally {
        c.disconnect()
      }
    } catch (e: Exception) {
      0
    }
  }

  companion object {
    const val CLIENT_ID = "plbls-android"
    private val LOCK = Any()
    private const val KEYSTORE = "AndroidKeyStore"
    private const val ALIAS = "estompe-session"
    private const val AES_GCM = "AES/GCM/NoPadding"
    private const val IV = 12
    private const val CLAIR = "refresh"
    private const val CHIFFRE = "refresh_chiffre"
  }
}
