package fr.plbls.estompe

import android.app.Notification
import android.app.Person
import android.app.RemoteInput
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.Parcelable
import android.provider.Telephony
import android.service.notification.NotificationListenerService
import android.service.notification.StatusBarNotification
import androidx.core.app.NotificationManagerCompat
import java.text.Collator
import java.util.Locale

data class Pending(val key: String, val app: String, val label: String, val sender: String, val text: String, val time: Long, val replyable: Boolean)

enum class NoteReply { SENT, GONE, FAILED }

const val UNKNOWN_APP = "une application"

fun groupConversation(flagged: Boolean, conversationTitle: String?, senders: List<String>): Boolean =
  flagged || !conversationTitle.isNullOrBlank() || senders.map { fold(it) }.filter { it.isNotEmpty() }.distinct().size > 1

fun sameSender(expected: String, senders: List<String>): Boolean = fold(expected).let { e -> e.isNotEmpty() && senders.any { fold(it) == e } }

class AssistantListener : NotificationListenerService() {
  override fun onListenerConnected() {
    super.onListenerConnected()
    instance = this
  }

  override fun onListenerDisconnected() {
    if (instance === this) instance = null
    try { requestRebind(ComponentName(this, AssistantListener::class.java)) } catch (e: Exception) { DriveService.note("écoute des notifications : reconnexion $e") }
    super.onListenerDisconnected()
  }

  override fun onDestroy() {
    if (instance === this) instance = null
    super.onDestroy()
  }

  private fun active(): Array<StatusBarNotification> = try { activeNotifications ?: emptyArray() } catch (e: Exception) {
    DriveService.note("écoute des notifications : lecture $e")
    emptyArray()
  }

  private fun label(pkg: String): String = labels.getOrPut(pkg) {
    try { packageManager.getApplicationLabel(packageManager.getApplicationInfo(pkg, 0)).toString().trim().ifEmpty { UNKNOWN_APP } } catch (e: Exception) { UNKNOWN_APP }
  }

  private fun collect(apps: Set<String>): List<Pending> {
    val out = ArrayList<Pending>()
    for (sbn in active()) {
      try {
        if (sbn.packageName !in apps || !eligible(sbn)) continue
        val n = sbn.notification
        val extras = n.extras ?: continue
        val replyable = replyAction(n) != null
        val label = label(sbn.packageName)
        val messages = styleMessages(extras)
        if (messages != null) {
          if (group(extras, messages)) continue
          messages.forEachIndexed { i, m ->
            val key = sbn.key + "#" + if (m.time > 0) m.time.toString() else "r$i"
            out.add(Pending(key, sbn.packageName, label, m.sender, m.text, if (m.time > 0) m.time else sbn.postTime, replyable))
          }
        } else if (n.category == Notification.CATEGORY_MESSAGE) {
          val text = (extras.getCharSequence(Notification.EXTRA_BIG_TEXT) ?: extras.getCharSequence(Notification.EXTRA_TEXT))?.toString()?.trim().orEmpty()
          val sender = extras.getCharSequence(Notification.EXTRA_TITLE)?.toString()?.trim().orEmpty()
          if (text.isEmpty()) continue
          val time = if (n.`when` > 0) n.`when` else sbn.postTime
          out.add(Pending(sbn.key + "#" + time, sbn.packageName, label, sender.ifEmpty { label }, text, time, replyable))
        }
      } catch (e: Exception) {
        DriveService.note("écoute des notifications : notification illisible (${e.javaClass.simpleName})")
      }
    }
    return out.sortedBy { it.time }
  }

  private fun eligible(sbn: StatusBarNotification): Boolean {
    if (sbn.packageName == packageName || sbn.isOngoing) return false
    val n = sbn.notification ?: return false
    if (n.flags and Notification.FLAG_GROUP_SUMMARY != 0) return false
    return n.extras?.containsKey(Notification.EXTRA_MESSAGES) == true || n.category == Notification.CATEGORY_MESSAGE
  }

  private fun sendReply(key: String, sender: String, text: String): NoteReply {
    val all = active()
    val sbn = all.firstOrNull { it.key == key.substringBeforeLast('#') } ?: all.firstOrNull { it.key == key } ?: return NoteReply.GONE
    val n = sbn.notification ?: return NoteReply.GONE
    val extras = n.extras ?: return NoteReply.GONE
    val messages = styleMessages(extras)
    val senders = if (messages != null) { if (group(extras, messages)) return NoteReply.GONE; messages.map { it.sender } }
      else listOf(extras.getCharSequence(Notification.EXTRA_TITLE)?.toString()?.trim().orEmpty().ifEmpty { label(sbn.packageName) })
    if (!sameSender(sender, senders)) return NoteReply.GONE
    val action = replyAction(n) ?: return NoteReply.GONE
    val inputs = action.remoteInputs ?: return NoteReply.GONE
    return try {
      val results = Bundle()
      for (input in inputs) if (input.allowFreeFormInput) results.putCharSequence(input.resultKey, text)
      val intent = Intent()
      RemoteInput.addResultsToIntent(inputs, intent, results)
      if (Build.VERSION.SDK_INT >= 28) RemoteInput.setResultsSource(intent, RemoteInput.SOURCE_FREE_FORM_INPUT)
      action.actionIntent.send(this, 0, intent)
      NoteReply.SENT
    } catch (e: Exception) {
      DriveService.note("réponse à une notification : ${e.javaClass.simpleName}")
      NoteReply.FAILED
    }
  }

  private fun packagesWithMessages(): Set<String> = active().filter { eligible(it) }.map { it.packageName }.toSet()

  private class Line(val sender: String, val text: String, val time: Long)

  companion object {
    @Volatile private var instance: AssistantListener? = null
    private val labels = java.util.concurrent.ConcurrentHashMap<String, String>()
    private const val SHARE_MAX = 30

    fun enabled(c: Context): Boolean = try {
      NotificationManagerCompat.getEnabledListenerPackages(c).contains(c.packageName)
    } catch (e: Exception) { false }

    val connected get() = instance != null

    fun component(c: Context): ComponentName = ComponentName(c, AssistantListener::class.java)

    fun pending(apps: Set<String>): List<Pending> {
      if (apps.isEmpty()) return emptyList()
      val s = instance ?: return emptyList()
      return try { s.collect(apps) } catch (e: Exception) {
        DriveService.note("écoute des notifications : $e")
        emptyList()
      }
    }

    fun reply(key: String, sender: String, text: String): NoteReply {
      if (key.isEmpty() || text.isBlank()) return NoteReply.FAILED
      val s = instance ?: return NoteReply.GONE
      return try { s.sendReply(key, sender, text) } catch (e: Exception) {
        DriveService.note("réponse à une notification : ${e.javaClass.simpleName}")
        NoteReply.FAILED
      }
    }

    @Suppress("DEPRECATION")
    fun messagingApps(c: Context): List<MusicApp> {
      val pm = c.packageManager
      val found = LinkedHashMap<String, String>()
      val sms = try { Telephony.Sms.getDefaultSmsPackage(c) } catch (e: Exception) { null }
      val chosen = try { AssistantStore(c).load().apps } catch (e: Exception) { emptySet() }
      fun add(pkg: String, label: () -> String) {
        if (pkg == c.packageName || pkg in found || (pkg == sms && pkg !in chosen)) return
        found[pkg] = try { label().trim().ifEmpty { UNKNOWN_APP } } catch (e: Exception) { UNKNOWN_APP }
      }
      try {
        instance?.packagesWithMessages()?.forEach { pkg -> add(pkg) { pm.getApplicationLabel(pm.getApplicationInfo(pkg, 0)).toString() } }
      } catch (e: Exception) { DriveService.note("apps de messagerie : $e") }
      try {
        pm.queryIntentActivities(Intent(Intent.ACTION_SENDTO, Uri.parse("smsto:")), 0).forEach { ri ->
          ri.activityInfo?.let { a -> add(a.packageName) { a.applicationInfo.loadLabel(pm).toString() } }
        }
        pm.queryIntentActivities(Intent(Intent.ACTION_SEND).setType("text/plain"), 0).take(SHARE_MAX).forEach { ri ->
          ri.activityInfo?.let { a -> add(a.packageName) { a.applicationInfo.loadLabel(pm).toString() } }
        }
      } catch (e: Exception) { DriveService.note("apps de messagerie : $e") }
      val collator = Collator.getInstance(Locale.FRENCH)
      return found.map { (id, label) -> MusicApp(id, label) }.sortedWith(Comparator { a, b -> collator.compare(a.label, b.label) })
    }

    private fun replyAction(n: Notification): Notification.Action? {
      fun free(a: Notification.Action) = a.actionIntent != null && a.remoteInputs?.any { it.allowFreeFormInput } == true
      val actions = n.actions.orEmpty().filter { free(it) }
      if (Build.VERSION.SDK_INT >= 28) actions.firstOrNull { it.semanticAction == Notification.Action.SEMANTIC_ACTION_REPLY }?.let { return it }
      actions.firstOrNull()?.let { return it }
      return try { Notification.WearableExtender(n).actions.firstOrNull { free(it) } } catch (e: Exception) { null }
    }

    private fun group(extras: Bundle, messages: List<Line>): Boolean = groupConversation(
      Build.VERSION.SDK_INT >= 28 && extras.getBoolean(Notification.EXTRA_IS_GROUP_CONVERSATION, false),
      extras.getCharSequence(Notification.EXTRA_CONVERSATION_TITLE)?.toString(), messages.map { it.sender })

    private fun selfNames(extras: Bundle): Set<String> {
      val names = HashSet<String>()
      extras.getCharSequence(Notification.EXTRA_SELF_DISPLAY_NAME)?.let { names.add(fold(it.toString())) }
      if (Build.VERSION.SDK_INT >= 28) person(extras)?.name?.let { names.add(fold(it.toString())) }
      names.remove("")
      return names
    }

    private fun styleMessages(extras: Bundle): List<Line>? {
      val raw = parcelables(extras, Notification.EXTRA_MESSAGES) ?: return null
      val self = selfNames(extras)
      val out = ArrayList<Line>()
      if (Build.VERSION.SDK_INT >= 28) {
        for (m in Notification.MessagingStyle.Message.getMessagesFromBundleArray(raw)) {
          val text = m.text?.toString()?.trim().orEmpty()
          val sender = (m.senderPerson?.name ?: m.sender)?.toString()?.trim().orEmpty()
          if (text.isEmpty() || (m.senderPerson == null && m.sender == null) || fold(sender) in self) continue
          out.add(Line(sender.ifEmpty { title(extras) }, text, m.timestamp))
        }
      } else {
        for (p in raw) {
          val b = p as? Bundle ?: continue
          val text = b.getCharSequence("text")?.toString()?.trim().orEmpty()
          val sender = b.getCharSequence("sender") ?: continue
          if (text.isEmpty() || fold(sender.toString()) in self) continue
          out.add(Line(sender.toString().trim().ifEmpty { title(extras) }, text, b.getLong("time")))
        }
      }
      return out
    }

    private fun title(extras: Bundle): String = extras.getCharSequence(Notification.EXTRA_TITLE)?.toString()?.trim().orEmpty()

    @Suppress("DEPRECATION")
    private fun person(extras: Bundle): Person? = if (Build.VERSION.SDK_INT >= 28) extras.getParcelable(Notification.EXTRA_MESSAGING_PERSON) as? Person else null

    @Suppress("DEPRECATION")
    private fun parcelables(extras: Bundle, key: String): Array<Parcelable>? = extras.getParcelableArray(key)
  }
}
