package fr.plbls.estompe

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.RemoteInput
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.app.Person
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.ProcessLifecycleOwner
import com.google.firebase.messaging.FirebaseMessagingService
import com.google.firebase.messaging.RemoteMessage
import org.json.JSONObject
import kotlin.concurrent.thread

private const val REPLY = "reply"

/** « c:<id> » → salon, « d:<id> » → conversation privée (même convention que le serveur et le service worker). */
fun convPath(conv: String) = (if (conv.startsWith("c:")) "/channels/" else "/dms/") + conv.substring(2)

/** Un canal par type d'évènement, pour que chacun règle son importance et son son dans les réglages Android. */
fun ensureChannels(context: Context) {
  val m = context.getSystemService(NotificationManager::class.java)
  for ((id, name, importance) in listOf(
    Triple("dm", "Messages privés", NotificationManager.IMPORTANCE_HIGH),
    Triple("mention", "Mentions", NotificationManager.IMPORTANCE_HIGH),
    Triple("channel", "Messages des salons", NotificationManager.IMPORTANCE_DEFAULT),
    Triple("voice", "Activité vocale", NotificationManager.IMPORTANCE_LOW),
    Triple("drive", "Mode conduite", NotificationManager.IMPORTANCE_LOW),
  )) m.createNotificationChannel(NotificationChannel(id, name, importance))
}

/** Reçoit les messages FCM « data » (premier plan, arrière-plan, app fermée) et construit la notification. */
class EstompeMessagingService : FirebaseMessagingService() {
  override fun onNewToken(token: String) {
    val s = Session(this)
    val previous = s.fcmToken
    s.fcmToken = token
    // jeton renouvelé app fermée : on le réenregistre tout de suite (sinon au prochain lancement, par le front)
    thread { s.call("PUT", "/push/fcm", JSONObject().put("token", token).apply { previous?.let { put("previous", it) } }) }
  }

  override fun onMessageReceived(message: RemoteMessage) {
    val d = message.data
    // message de données « lu » : aucune notification à afficher, seulement à retirer (app fermée comprise)
    d["read"]?.let { return dismissRead(this, d["conv"].orEmpty(), it) }
    DriveService.read(this, d) // mode conduite : lecture à voix haute, même application au premier plan
    // app au premier plan : elle affiche déjà ses propres notifications
    if (ProcessLifecycleOwner.get().lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED)) return
    val m = Incoming.from(d) ?: return
    ensureChannels(this)
    // une notification par conversation (et non par message) : Android Auto la présente comme un fil
    val tag = if (m.replyable) "conv-${m.conv}" else m.tag
    val open = Intent(this, MainActivity::class.java).putExtra("open", d["url"])
      .addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP)
    val b = NotificationCompat.Builder(this, m.kind.takeIf { it in setOf("dm", "mention", "channel", "voice") } ?: "channel")
      .setSmallIcon(R.drawable.ic_notification).setColor(getColor(R.color.notification_accent))
      .setContentTitle(d["title"] ?: "Estompe")
      .setContentText(d["body"])
      .setStyle(NotificationCompat.BigTextStyle().bigText(d["body"]))
      .setAutoCancel(true)
      .setSilent(d["silent"] == "true")
      .setNumber(NotificationManagerCompat.from(this).activeNotifications.size + 1) // badge d'icône
      .setContentIntent(PendingIntent.getActivity(this, tag.hashCode(), open, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT))
    if (m.replyable) {
      b.setStyle(messagingStyle(NotificationManagerCompat.from(this), tag, m, d["body"].orEmpty()))
      fun action(name: String, flags: Int) = PendingIntent.getBroadcast(this, tag.hashCode(),
        Intent(this, NotificationActionReceiver::class.java).setAction(name).putExtra("conv", m.conv).putExtra("msg", m.msg).putExtra("tag", tag),
        PendingIntent.FLAG_UPDATE_CURRENT or flags)
      // sémantiques et « sans interface » : Android Auto n'affiche et ne déclenche que ces actions, sans toucher le téléphone
      b.addAction(NotificationCompat.Action.Builder(0, "Répondre", action(REPLY, PendingIntent.FLAG_MUTABLE)) // mutable : Android y insère le texte saisi
        .addRemoteInput(androidx.core.app.RemoteInput.Builder(REPLY).setLabel("Votre réponse…").build())
        .setSemanticAction(NotificationCompat.Action.SEMANTIC_ACTION_REPLY).setShowsUserInterface(false).build())
      b.addAction(NotificationCompat.Action.Builder(0, "Marquer comme lu", action("read", PendingIntent.FLAG_IMMUTABLE))
        .setSemanticAction(NotificationCompat.Action.SEMANTIC_ACTION_MARK_AS_READ).setShowsUserInterface(false).build())
    }
    if (NotificationManagerCompat.from(this).areNotificationsEnabled()) {
      try { NotificationManagerCompat.from(this).notify(tag, 0, b.build()) } catch (e: SecurityException) { /* permission retirée entre-temps */ }
    }
  }
}

/** Notification déjà affichée pour `tag`, s'il y en a une. */
private fun shown(nm: NotificationManagerCompat, tag: String) = nm.activeNotifications.firstOrNull { it.tag == tag && it.id == 0 }?.notification

/**
 * Fil de la conversation : l'historique déjà affiché plus ce message. Aperçu masqué : le serveur n'envoie ni auteur ni
 * contenu, le fil ne porte que « Nouveau message de X ».
 */
fun messagingStyle(nm: NotificationManagerCompat, tag: String, m: Incoming, body: String): NotificationCompat.MessagingStyle {
  val style = shown(nm, tag)?.let { NotificationCompat.MessagingStyle.extractMessagingStyleFromNotification(it) }
    ?: NotificationCompat.MessagingStyle(Person.Builder().setName("Vous").build()).also {
      it.isGroupConversation = m.room.isNotEmpty()
      if (m.room.isNotEmpty()) it.conversationTitle = m.room
    }
  val who = Person.Builder().setName(m.sender.ifEmpty { "Estompe" }).setKey(m.sender.ifEmpty { "estompe" }).build()
  // l'identifiant du message permet de ne garder que les non lus quand la conversation est lue ailleurs
  style.addMessage(NotificationCompat.MessagingStyle.Message(if (m.hidden) m.title else m.text.ifEmpty { body }, System.currentTimeMillis(), who)
    .also { it.extras.putString("msg", m.msg) })
  return style
}

/** Reconstruit la notification `current` avec un autre fil (même canal, intent et actions). */
private fun rebuilt(context: Context, current: Notification, style: NotificationCompat.MessagingStyle, text: CharSequence? = null): Notification {
  val b = NotificationCompat.Builder(context, NotificationCompat.getChannelId(current) ?: "dm").setSmallIcon(R.drawable.ic_notification).setColor(context.getColor(R.color.notification_accent))
    .setContentTitle(current.extras.getCharSequence(Notification.EXTRA_TITLE)).setContentText(text ?: current.extras.getCharSequence(Notification.EXTRA_TEXT))
    .setContentIntent(current.contentIntent).setAutoCancel(true).setStyle(style).setOnlyAlertOnce(true).setSilent(true)
  for (i in 0 until NotificationCompat.getActionCount(current)) NotificationCompat.getAction(current, i)?.let { b.addAction(it) }
  return b.build()
}

/**
 * Conversation lue (sur ce téléphone si `last` est nul, sinon ailleurs jusqu'au message `last`) : retire sa notification,
 * ou la réduit aux messages arrivés après la lecture.
 */
fun dismissRead(context: Context, conv: String, last: String?) {
  val nm = NotificationManagerCompat.from(context)
  val tag = "conv-$conv"
  val current = shown(nm, tag)
  val fil = current?.let { NotificationCompat.MessagingStyle.extractMessagingStyleFromNotification(it) }
  val lastId = last?.toLongOrNull()
  val unread = if (lastId == null) emptyList() else fil?.messages.orEmpty().filter { (it.extras.getString("msg")?.toLongOrNull() ?: 0) > lastId }
  DriveService.note("lu $conv jusqu'à ${last ?: "—"} : ${if (current == null) "aucune notification" else if (unread.isEmpty()) "retirée" else "${unread.size} non lu(s) conservé(s)"}")
  if (current == null) return
  if (fil == null || unread.isEmpty()) return nm.cancel(tag, 0)
  val rest = NotificationCompat.MessagingStyle(fil.user).also { st ->
    st.isGroupConversation = fil.isGroupConversation
    st.conversationTitle = fil.conversationTitle
    unread.forEach { st.addMessage(it) }
  }
  try { nm.notify(tag, 0, rebuilt(context, current, rest, unread.last().text)) } catch (e: SecurityException) { /* permission retirée */ }
}

/** Actions « Répondre » et « Marquer comme lu » : appel de l'API directement, sans ouvrir l'app. */
class NotificationActionReceiver : BroadcastReceiver() {
  override fun onReceive(context: Context, intent: Intent) {
    val conv = intent.getStringExtra("conv") ?: return
    val tag = intent.getStringExtra("tag") ?: return
    val text = RemoteInput.getResultsFromIntent(intent)?.getCharSequence(REPLY)?.toString()?.trim()
    val pending = goAsync()
    thread {
      val s = Session(context)
      val code = when {
        intent.action == REPLY && !text.isNullOrEmpty() ->
          s.call("POST", convPath(conv) + "/messages", JSONObject().put("content", text).put("reply_to", intent.getStringExtra("msg")))
        intent.action == "read" -> s.call("POST", convPath(conv) + "/read")
        else -> -1
      }
      val nm = NotificationManagerCompat.from(context)
      val current = shown(nm, tag)
      val fil = current?.let { NotificationCompat.MessagingStyle.extractMessagingStyleFromNotification(it) }
      if (code in 200..299 && intent.action == REPLY && current != null && fil != null) {
        // la réponse rejoint le fil (et arrête l'indicateur de saisie) ; la notification disparaît au « marquer comme lu »
        fil.addMessage(text.orEmpty(), System.currentTimeMillis(), null as Person?)
        try { nm.notify(tag, 0, rebuilt(context, current, fil)) } catch (e: SecurityException) { /* permission retirée */ }
      } else if (code in 200..299) nm.cancel(tag, 0)
      else if (code != -1) { // échec : la notification reste, avec la raison (l'indicateur de saisie ne doit pas tourner indéfiniment)
        val failed = NotificationCompat.Builder(context, "dm").setSmallIcon(R.drawable.ic_notification).setColor(context.getColor(R.color.notification_accent))
          .setContentTitle("Estompe").setContentText(if (code == 0) "Action impossible : ouvre l'application." else "Action refusée ($code).")
          .setAutoCancel(true).build()
        try { nm.notify(tag, 0, failed) } catch (e: SecurityException) { /* permission retirée */ }
      }
      pending.finish()
    }
  }
}
