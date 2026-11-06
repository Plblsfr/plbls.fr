// Notifications push : importé par le service worker généré (voir vite.config.ts).
importScripts('/config.js')
const API = self.__ESTOMPE__?.apiUrl || ''

self.addEventListener('push', (event) => {
  let d = {}
  try { d = event.data ? event.data.json() : {} } catch { /* corps illisible */ }
  // Actions rapides : seulement pour un message (pas pour l'activité vocale).
  const actions = d.conv && d.msg
    ? [{ action: 'reply', type: 'text', title: 'Répondre', placeholder: 'Votre réponse…' }, { action: 'read', title: 'Marquer comme lu' }]
    : []
  // Toujours afficher : Safari révoque l'abonnement d'un push qui n'affiche rien. Le serveur n'envoie rien à une application connectée.
  event.waitUntil(self.registration.showNotification(d.title || 'Estompe', {
    body: d.body || '', tag: d.tag, silent: !!d.silent, actions,
    data: { url: d.url || '/', conv: d.conv, msg: d.msg }, icon: '/icon.svg', badge: '/icon.svg',
  }))
})

const call = (path, body) => fetch(API + path, {
  method: 'POST', credentials: 'include',
  ...(body ? { headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) } : {}),
})

/** « c:<id> » → salon, « d:<id> » → conversation privée. */
const convPath = (conv) => (conv.startsWith('c:') ? '/channels/' : '/dms/') + conv.slice(2)

self.addEventListener('notificationclick', (event) => {
  event.notification.close()
  const { url = '/', conv, msg } = event.notification.data || {}
  const reply = (event.reply || '').trim()
  if (conv && event.action === 'read') return event.waitUntil(call(convPath(conv) + '/read').catch(() => {}))
  // Réponse saisie dans la notification (navigateurs qui la proposent) ; sinon le clic ouvre la conversation.
  if (conv && event.action === 'reply' && reply) {
    return event.waitUntil(call(convPath(conv) + '/messages', { content: reply, reply_to: msg }).catch(() => {}))
  }
  event.waitUntil(
    self.clients.matchAll({ type: 'window', includeUncontrolled: true }).then((wins) => {
      const w = wins[0]
      if (!w) return self.clients.openWindow(url)
      // Hash seul : la page change de conversation sans recharger.
      return w.navigate(url).catch(() => w).then((c) => (c || w).focus())
    }),
  )
})
