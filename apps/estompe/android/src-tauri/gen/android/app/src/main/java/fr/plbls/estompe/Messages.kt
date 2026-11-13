package fr.plbls.estompe

import android.Manifest
import android.app.Activity
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.PackageManager
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.provider.BaseColumns
import android.provider.ContactsContract
import android.provider.Telephony
import android.telephony.SmsManager
import androidx.core.content.ContextCompat
import java.util.UUID

enum class Sent { OK, FAILED, UNKNOWN }

fun speakableNumber(address: String): String {
  val raw = address.trim()
  if (raw.isEmpty()) return "numéro inconnu"
  if (raw.any { it.isLetter() }) return raw
  var digits = raw.filter { it.isDigit() }
  if (digits.isEmpty()) return raw
  if (raw.startsWith("+33") && digits.length == 11) digits = "0" + digits.substring(2)
  else if (raw.startsWith("0033") && digits.length == 13) digits = "0" + digits.substring(4)
  val size = if (digits.length == 10 && digits.startsWith("0")) 2 else 3
  if (digits.length <= 6) return digits.chunked(2).joinToString(" ")
  val head = if (raw.startsWith("+") && size == 3) "plus " else ""
  return head + digits.chunked(size).joinToString(" ")
}

class Phone(context: Context) {
  private val app = context.applicationContext
  private val prefs = app.getSharedPreferences("assistant", Context.MODE_PRIVATE)

  private fun granted(permission: String) = try {
    ContextCompat.checkSelfPermission(app, permission) == PackageManager.PERMISSION_GRANTED
  } catch (e: Exception) { false }

  fun canReadSms(): Boolean = granted(Manifest.permission.READ_SMS)
  fun canSendSms(): Boolean = granted(Manifest.permission.SEND_SMS)
  fun canReadContacts(): Boolean = granted(Manifest.permission.READ_CONTACTS)

  fun contacts(): List<Contact> {
    if (!canReadContacts()) return emptyList()
    val people = LinkedHashMap<Long, Pair<String, MutableList<String>>>()
    try {
      val projection = arrayOf(ContactsContract.CommonDataKinds.Phone.CONTACT_ID, ContactsContract.CommonDataKinds.Phone.DISPLAY_NAME,
        ContactsContract.CommonDataKinds.Phone.NUMBER)
      app.contentResolver.query(ContactsContract.CommonDataKinds.Phone.CONTENT_URI, projection, null, null,
        ContactsContract.CommonDataKinds.Phone.DISPLAY_NAME + " ASC")?.use { c ->
        while (c.moveToNext()) {
          val name = c.getString(1)?.trim().orEmpty()
          val number = c.getString(2)?.trim().orEmpty()
          if (name.isEmpty() || number.isEmpty()) continue
          val numbers = people.getOrPut(c.getLong(0)) { name to ArrayList() }.second
          if (numbers.none { samePhone(it, number) }) numbers.add(number)
        }
      }
    } catch (e: Exception) {
      DriveService.note("lecture des contacts : $e")
      return emptyList()
    }
    return people.values.map { (name, numbers) -> Contact(name, numbers) }
  }

  fun unreadSms(): List<Sms> {
    if (!canReadSms()) return emptyList()
    val spoken = spokenIds()
    val found = ArrayList<Triple<Long, String, Pair<String, Long>>>()
    try {
      app.contentResolver.query(Telephony.Sms.Inbox.CONTENT_URI, COLUMNS,
        Telephony.TextBasedSmsColumns.READ + " = 0 AND " + Telephony.TextBasedSmsColumns.DATE + " > ?", arrayOf((System.currentTimeMillis() - UNREAD_AGE_MS).toString()),
        Telephony.TextBasedSmsColumns.DATE + " DESC")?.use { c ->
        var n = 0
        while (n < UNREAD_MAX && c.moveToNext()) {
          n++
          val id = c.getLong(0)
          if (id in spoken) continue
          val body = c.getString(2).orEmpty()
          if (body.isBlank()) continue
          found.add(Triple(id, c.getString(1).orEmpty(), body to c.getLong(3)))
        }
      }
    } catch (e: Exception) {
      DriveService.note("lecture des SMS non lus : $e")
      return emptyList()
    }
    if (found.isEmpty()) return emptyList()
    val book = contacts()
    return found.map { (id, address, rest) -> Sms(id, address, senderName(address, book), rest.first, rest.second) }
      .sortedWith(compareBy<Sms>({ it.date }, { it.id }))
  }

  fun smsFrom(contact: Contact, max: Int = 5): List<Sms> {
    if (!canReadSms() || max <= 0 || contact.numbers.isEmpty()) return emptyList()
    val found = ArrayList<Sms>()
    try {
      app.contentResolver.query(Telephony.Sms.Inbox.CONTENT_URI, COLUMNS, null, null, Telephony.TextBasedSmsColumns.DATE + " DESC")?.use { c ->
        var n = 0
        while (found.size < max && n < SCAN_MAX && c.moveToNext()) {
          n++
          val address = c.getString(1).orEmpty()
          if (address.isEmpty() || contact.numbers.none { samePhone(it, address) }) continue
          val body = c.getString(2).orEmpty()
          if (body.isBlank()) continue
          found.add(Sms(c.getLong(0), address, contact.name, body, c.getLong(3)))
        }
      }
    } catch (e: Exception) {
      DriveService.note("lecture des SMS d'un contact : $e")
      return emptyList()
    }
    return found.sortedWith(compareBy<Sms>({ it.date }, { it.id }))
  }

  fun markSpoken(ids: Collection<Long>) {
    if (ids.isEmpty()) return
    try {
      synchronized(LOCK) {
        val all = (spokenIds() + ids).sortedDescending().take(SPOKEN_MAX)
        prefs.edit().putString(SPOKEN_KEY, all.joinToString(",")).apply()
      }
    } catch (e: Exception) { DriveService.note("mémoire des SMS lus : $e") }
  }

  fun send(number: String, text: String, done: (Sent) -> Unit) {
    val ui = Handler(Looper.getMainLooper())
    ui.post { sendNow(ui, number.trim(), text, done) }
  }

  private fun sendNow(ui: Handler, number: String, text: String, done: (Sent) -> Unit) {
    var over = false
    var receiver: BroadcastReceiver? = null
    var timeout: Runnable? = null
    fun finish(ok: Sent) {
      if (over) return
      over = true
      timeout?.let { ui.removeCallbacks(it) }
      receiver?.let { r -> try { app.unregisterReceiver(r) } catch (e: Exception) { DriveService.note("envoi SMS : désenregistrement ${e.javaClass.simpleName}") } }
      receiver = null
      try { done(ok) } catch (e: Exception) { DriveService.note("envoi SMS : rappel ${e.javaClass.simpleName}") }
    }
    if (number.isEmpty() || text.isBlank()) return finish(Sent.FAILED)
    if (!canSendSms()) { DriveService.note("envoi SMS : permission absente"); return finish(Sent.FAILED) }
    try {
      val sms = smsManager() ?: run { DriveService.note("envoi SMS : service indisponible"); return finish(Sent.FAILED) }
      val parts = sms.divideMessage(text)
      if (parts.isNullOrEmpty()) return finish(Sent.FAILED)
      val action = app.packageName + ".SMS_SENT." + UUID.randomUUID()
      val accepted = HashSet<Int>()
      val r = object : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) {
          if (intent.action != action) return
          if (resultCode != Activity.RESULT_OK) { DriveService.note("envoi SMS : refus du système (code $resultCode)"); return finish(if (parts.size == 1) Sent.FAILED else Sent.UNKNOWN) }
          accepted.add(intent.getIntExtra(EXTRA_PART, -1))
          if (accepted.size >= parts.size) finish(Sent.OK)
        }
      }
      if (Build.VERSION.SDK_INT >= 33) app.registerReceiver(r, IntentFilter(action), Context.RECEIVER_NOT_EXPORTED)
      else app.registerReceiver(r, IntentFilter(action))
      receiver = r
      val t = Runnable { DriveService.note("envoi SMS : pas d'accusé d'envoi"); finish(Sent.UNKNOWN) }
      timeout = t
      ui.postDelayed(t, SEND_TIMEOUT_MS)
      val flags = PendingIntent.FLAG_ONE_SHOT or PendingIntent.FLAG_IMMUTABLE
      val sent = ArrayList<PendingIntent>(parts.size)
      for (i in parts.indices) {
        sent.add(PendingIntent.getBroadcast(app, i, Intent(action).setPackage(app.packageName).putExtra(EXTRA_PART, i), flags))
      }
      sms.sendMultipartTextMessage(number, null, parts, sent, null)
    } catch (e: Exception) {
      DriveService.note("envoi SMS : ${e.javaClass.simpleName}")
      finish(Sent.FAILED)
    }
  }

  @Suppress("DEPRECATION")
  private fun smsManager(): SmsManager? =
    if (Build.VERSION.SDK_INT >= 31) app.getSystemService(SmsManager::class.java) else SmsManager.getDefault()

  private fun spokenIds(): Set<Long> = try {
    prefs.getString(SPOKEN_KEY, "").orEmpty().split(',').mapNotNull { it.toLongOrNull() }.toHashSet()
  } catch (e: Exception) { emptySet() }

  private fun senderName(address: String, book: List<Contact>): String =
    book.firstOrNull { c -> c.numbers.any { samePhone(it, address) } }?.name ?: speakable(address)

  companion object {
    private val LOCK = Any()
    private const val SPOKEN_KEY = "spokenSms"
    private const val SPOKEN_MAX = 500
    private const val UNREAD_MAX = 50
    private const val UNREAD_AGE_MS = 7 * 86_400_000L
    private const val SCAN_MAX = 1000
    private const val SEND_TIMEOUT_MS = 30_000L
    private const val EXTRA_PART = "part"
    private val COLUMNS = arrayOf(BaseColumns._ID, Telephony.TextBasedSmsColumns.ADDRESS, Telephony.TextBasedSmsColumns.BODY,
      Telephony.TextBasedSmsColumns.DATE)

    fun speakable(address: String): String = speakableNumber(address)
  }
}
