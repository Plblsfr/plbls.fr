package fr.plbls.estompe

import android.content.Context
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL
import java.net.URLEncoder

/**
 * Session de l'app côté natif : le refresh token (qui tourne à chaque usage) vit ici, partagé entre le front
 * (via EstompeNative) et les actions de notification, qui appellent l'API sans WebView.
 */
class Session(context: Context) {
  private val prefs = context.applicationContext.getSharedPreferences("session", Context.MODE_PRIVATE)

  var refreshToken: String?
    get() = prefs.getString("refresh", null)
    set(v) { prefs.edit().apply { if (v == null) remove("refresh") else putString("refresh", v) }.apply() }

  var fcmToken: String?
    get() = prefs.getString("fcm", null)
    set(v) { prefs.edit().putString("fcm", v).apply() }

  val apiUrl: String? get() = prefs.getString("api", null)

  fun configure(issuer: String, apiUrl: String) = prefs.edit().putString("issuer", issuer).putString("api", apiUrl).apply()

  private fun form(vararg kv: Pair<String, String>) = kv.joinToString("&") { "${it.first}=${URLEncoder.encode(it.second, "UTF-8")}" }

  /** Jeton d'accès frais (un seul renouvellement à la fois : le refresh token tourne) ; null sans session. */
  @Synchronized
  private fun accessToken(): String? {
    val rt = refreshToken ?: return null
    val issuer = prefs.getString("issuer", null) ?: return null
    val c = URL("$issuer/protocol/openid-connect/token").openConnection() as HttpURLConnection
    try {
      c.requestMethod = "POST"
      c.doOutput = true
      c.connectTimeout = 10_000
      c.readTimeout = 10_000
      c.setRequestProperty("Content-Type", "application/x-www-form-urlencoded")
      c.outputStream.use { it.write(form("client_id" to CLIENT_ID, "grant_type" to "refresh_token", "refresh_token" to rt).toByteArray()) }
      if (c.responseCode == 400 || c.responseCode == 401) { refreshToken = null; return null } // session terminée côté Keycloak
      if (c.responseCode != 200) return null
      val j = JSONObject(c.inputStream.bufferedReader().readText())
      j.optString("refresh_token").takeIf { it.isNotEmpty() }?.let { refreshToken = it }
      return j.getString("access_token")
    } finally {
      c.disconnect()
    }
  }

  /** Appel authentifié de l'API ; renvoie le code HTTP, ou 0 sans session ou en cas d'échec réseau. */
  fun call(method: String, path: String, body: JSONObject = JSONObject()): Int {
    val api = prefs.getString("api", null) ?: return 0
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
  }
}
