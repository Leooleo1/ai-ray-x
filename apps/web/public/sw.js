// raiox on a paired device: shows the notes raiox sends when an AI finishes,
// and opens the conversation on a tap. It caches nothing; the panel always
// comes from the computer.
const LEADS = {
  pt: { proposal: 'Esperando sua aprovação', draft: 'Rascunho pronto para revisar', failed: 'Não deu certo', test: 'Avisos ligados neste aparelho' },
  en: { proposal: 'Waiting for your approval', draft: 'Draft ready to review', failed: 'It did not work', test: 'Notifications are on for this device' },
}

self.addEventListener('push', (event) => {
  let note = {}
  try {
    note = event.data ? event.data.json() : {}
  } catch {
    note = {}
  }
  const leads = (self.navigator.language || '').toLowerCase().startsWith('pt') ? LEADS.pt : LEADS.en
  const body = [leads[note.kind], note.body].filter(Boolean).join(' · ')
  event.waitUntil(
    self.registration.showNotification(note.title || 'raiox', {
      body,
      icon: '/icons/icon-192.png',
      badge: '/icons/icon-192.png',
      tag: note.tag,
      data: { url: typeof note.url === 'string' && note.url.startsWith('/') ? note.url : '/' },
    }),
  )
})

self.addEventListener('notificationclick', (event) => {
  event.notification.close()
  const url = event.notification.data?.url || '/'
  event.waitUntil(
    self.clients.matchAll({ type: 'window', includeUncontrolled: true }).then((open) => {
      const client = open.find((c) => 'focus' in c)
      if (!client) return self.clients.openWindow(url)
      return client.navigate(url).then((moved) => (moved || client).focus())
    }),
  )
})
