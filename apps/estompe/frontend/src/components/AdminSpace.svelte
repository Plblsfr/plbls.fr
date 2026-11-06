<script lang="ts">
  import { confirmer } from '../lib/confirm.svelte'
  import { onMount } from 'svelte'
  import BadgeAdmin from '../blocks/badges/BadgeAdmin.svelte'
  import AdminReports from '../blocks/signalements/AdminReports.svelte'
  import Icon from './Icon.svelte'
  import { api, filmsApi } from '../lib/api'
  import { reasonLabel, type FilmReject } from '../blocks/films/films'
  import { ADMIN_SECTIONS, adminSection } from '../lib/admin'
  import type { AdminBadge, AdminError, AdminLive, AdminLogEntry, AdminOverview, AdminStats, AdminUser } from '../lib/types'

  let { section, onmenu }: { section?: string; onmenu: () => void } = $props()
  const current = $derived(adminSection(section))
  const title = $derived(ADMIN_SECTIONS.find((s) => s.id === current)?.label ?? '')

  const UPDATES = 'https://estompe.plbls.fr/updates'
  let error = $state('')
  let info = $state('')
  let query = $state('')
  let users = $state<AdminUser[]>([])
  let ov = $state<AdminOverview | null>(null)
  let badges = $state<AdminBadge[]>([])
  let bot = $state({ releases: [] as string[], to: '', content: '' })
  let stats = $state<AdminStats | null>(null)
  let live = $state<AdminLive | null>(null)
  let installers = $state({ windows: '…', android: '…' })
  let log = $state<AdminLogEntry[]>([])
  let errors = $state<AdminError[]>([])
  let rejects = $state<FilmReject[]>([])

  const fail = (e: unknown) => (error = String((e as Error).message ?? e))
  const load = (f: () => Promise<unknown>) => f().then(() => (error = '')).catch(fail)
  const act = (f: () => Promise<unknown>, reload: () => Promise<unknown>) => load(async () => { await f(); await reload() })
  const when = (d: string) => new Date(d).toLocaleString('fr-FR')
  const yes = (b: boolean) => (b ? 'configuré' : 'non configuré')
  const mb = (n: number) => `${Math.round(n / (1 << 20))} Mo`
  const version = (file: string) =>
    fetch(`${UPDATES}/${file}`).then((r) => r.json()).then((j) => String(j.version ?? '—')).catch(() => '—')

  const loadUsers = () => api.adminUsers(query.trim()).then((l) => (users = l))
  const loadServers = () => api.adminOverview().then((o) => (ov = o))
  const loadBadges = () => api.adminBadges().then((l) => (badges = l))
  const loadBot = () => api.adminBot().then((b) => (bot.releases = b.releases))
  const loadLog = async () => { log = await api.adminLog(); errors = await api.adminErrors() }

  const suspend = async (u: AdminUser) =>
    (u.suspended || (await confirmer({ titre: 'Suspendre le compte', message: `Suspendre ${u.pseudo} ? Il ne pourra plus utiliser Estompe.`, action: 'Suspendre', destructif: true }))) && act(() => api.suspendUser(u.id, !u.suspended), loadUsers)
  const dropServer = async (id: string, name: string) =>
    (await confirmer({ titre: 'Supprimer le serveur', message: `Supprimer définitivement le serveur « ${name} » et tous ses messages ?`, action: 'Supprimer le serveur', destructif: true, saisie: { label: 'Saisissez le nom du serveur pour confirmer', exiger: name } })) && act(() => api.adminDeleteServer(id), loadServers)
  const sendBot = async () => {
    const target = bot.to.trim() ? bot.to.trim() : 'tous les comptes'
    if (!(await confirmer({ titre: 'Envoyer le message', message: `Envoyer ce message en privé à ${target} ?`, action: 'Envoyer' }))) return
    void act(async () => { const r = await api.botMessage(bot.content.trim(), bot.to.trim()); info = `${r.sent} message(s) envoyé(s)`; bot.content = '' }, loadBot)
  }

  $effect(() => {
    info = ''
    if (current === 'users') void load(loadUsers)
    else if (current === 'servers') void load(loadServers)
    else if (current === 'badges') void load(loadBadges)
    else if (current === 'bot') void load(loadBot)
    else if (current === 'logs') void load(loadLog)
    else if (current === 'films') void load(() => filmsApi.rejects().then((r) => (rejects = r.items)))
    else if (current === 'stats') {
      void load(async () => { stats = await api.adminStats(); await loadServers() })
      void version('latest.json').then((v) => (installers.windows = v))
      void version('android.json').then((v) => (installers.android = v))
      const poll = () => api.adminLive().then((l) => (live = l)).catch(fail)
      void poll()
      const t = setInterval(poll, 5000)
      return () => clearInterval(t)
    }
  })
</script>

<header class="main-head">
  <button class="ib burger" onclick={onmenu} aria-label="Menu"><Icon name="menu" /></button>
  <h1><span class="muted">Estompe ›</span> {title}</h1>
</header>
<div class="adm-page">
  {#if error}<p class="small" style="color: var(--danger)" role="alert">{error}</p>{/if}
  {#if info}<p class="small" role="status">{info}</p>{/if}

  {#if current === 'users'}
    <form class="row" onsubmit={(e) => { e.preventDefault(); void load(loadUsers) }}>
      <input class="plbls-input" type="search" placeholder="Rechercher un pseudo" aria-label="Rechercher un pseudo" bind:value={query} />
      <button class="plbls-button plbls-button--compact">Rechercher</button>
    </form>
    <ul class="adm">
      {#each users as u (u.id)}
        <li>
          <strong>{u.pseudo}</strong>{#if u.suspended} <span class="plbls-badge">suspendu</span>{/if}
          <span class="muted small">inscrit le {new Date(u.created_at).toLocaleDateString('fr-FR')} · {u.servers} serveur(s) · {u.messages} message(s)</span>
          <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => suspend(u)}>{u.suspended ? 'Rétablir' : 'Suspendre'}</button>
        </li>
      {:else}<li class="muted">Aucun résultat</li>{/each}
    </ul>

  {:else if current === 'servers'}
    <ul class="adm">
      {#each ov?.servers ?? [] as s (s.id)}
        <li>
          <strong>{s.name}</strong>
          <span class="muted small">{s.members} membres · {s.channels} salons · {s.messages} messages{s.last_message_at ? ` · dernier ${when(s.last_message_at)}` : ''}</span>
          <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => dropServer(s.id, s.name)}>Supprimer</button>
        </li>
      {:else}<li class="muted">Aucun serveur</li>{/each}
    </ul>

  {:else if current === 'badges'}
    <BadgeAdmin {badges} reload={loadBadges} {fail} />

  {:else if current === 'films'}
    <p class="small muted">Ces fichiers restent invisibles et illisibles dans Estompe. Une ligne par fichier et par raison.</p>
    <div class="film-table">
      <table>
        <thead><tr><th scope="col">Nom</th><th scope="col">Raison</th><th scope="col">Dernière détection</th></tr></thead>
        <tbody>
          {#each rejects as r (r.name + r.reason)}
            <tr><td><strong>{r.name}</strong></td><td>{reasonLabel(r.reason)}</td><td class="muted">{when(r.seen_at)}</td></tr>
          {:else}<tr><td colspan="3" class="muted">Aucun fichier écarté</td></tr>{/each}
        </tbody>
      </table>
    </div>

  {:else if current === 'reports'}
    <AdminReports {fail} />

  {:else if current === 'bot'}
    <p class="small muted">Messages privés envoyés par le compte « Estompe » (lecture seule pour les utilisateurs).</p>
    <form class="stack" onsubmit={(e) => { e.preventDefault(); sendBot() }}>
      <input class="plbls-input" placeholder="Pseudo du destinataire (vide : tous les comptes)" aria-label="Destinataire" bind:value={bot.to} maxlength="20" />
      <textarea class="plbls-input" rows="4" placeholder="Message" aria-label="Message" bind:value={bot.content} required maxlength="4000"></textarea>
      <button class="plbls-button">Envoyer</button>
    </form>
    <h3>Notes de version envoyées</h3>
    <ul class="adm">{#each bot.releases as v}<li>{v}</li>{:else}<li class="muted">Aucune</li>{/each}</ul>

  {:else if current === 'stats'}
    {#if stats}
      <ul class="adm">
        <li>{stats.users} compte(s) dont {stats.new_users_7d} cette semaine · {stats.suspended} suspendu(s)</li>
        <li>{stats.servers} serveur(s) · {stats.messages} message(s) dont {stats.messages_24h} ces dernières 24 h</li>
        <li>{stats.files} fichier(s) · {mb(stats.storage_bytes)} stockés</li>
        <li>{stats.errors_24h} erreur(s) serveur ces dernières 24 h</li>
      </ul>
    {/if}
    <h3>En direct</h3>
    {#if live}
      <p class="small muted">{live.connections} connexion(s) temps réel, {live.online.length} appareil(s) connecté(s)</p>
      <ul class="adm">
        {#each live.online as o}<li>{o.pseudo} <span class="muted small">{o.platform || '?'} {o.version}</span></li>{:else}<li class="muted">Personne</li>{/each}
      </ul>
      <h3>Salons vocaux actifs</h3>
      <ul class="adm">
        {#each live.voice as v}
          <li>{v.server} › {v.channel}
            <ul>{#each v.participants as p}<li>{p.pseudo}{p.screen ? ' · écran' : ''}{p.camera ? ' · caméra' : ''}</li>{/each}</ul>
          </li>
        {:else}<li class="muted">Aucun</li>{/each}
      </ul>
    {:else}<p class="muted small">Chargement…</p>{/if}
    {#if ov}
      <h3>Configuration</h3>
      <ul class="adm">
        <li>Notifications push (web) : {yes(ov.config.settings.push_web)}</li>
        <li>Notifications push (Android) : {yes(ov.config.settings.push_android)}</li>
        <li>GIPHY : {yes(ov.config.settings.giphy)}</li>
        <li>Métriques : {ov.config.settings.metrics ? 'activées' : 'désactivées'}</li>
        <li>LiveKit : {ov.config.settings.livekit_url} · {ov.config.max_participants} participants max · caméra et écran {ov.config.settings.allow_video ? 'autorisés' : 'désactivés'}{ov.config.screen_max_fps ? ` · écran ${ov.config.screen_max_height}p, ${ov.config.screen_max_fps} i/s` : ''}</li>
        <li>Stockage : {Math.round(ov.config.settings.max_storage / (1 << 30))} Go · fichiers {mb(ov.config.max_file_size)} max</li>
        <li>Origines autorisées : {(ov.config.settings.origins ?? []).join(', ') || '—'}</li>
      </ul>
      <h3>Versions</h3>
      <ul class="adm">
        <li>Backend : {ov.version}</li>
        <li>Front : {__APP_VERSION__}</li>
        <li>Installeur Windows : {installers.windows}</li>
        <li>Application Android : {installers.android}</li>
      </ul>
    {/if}

  {:else if current === 'logs'}
    <h3>Actions d'administration</h3>
    <ul class="adm">
      {#each log as l}<li><span class="muted small">{when(l.at)}</span> <strong>{l.actor}</strong> · {l.action}{l.target ? ` · ${l.target}` : ''}</li>{:else}<li class="muted">Aucune</li>{/each}
    </ul>
    <h3>Erreurs serveur (30 jours)</h3>
    <ul class="adm">
      {#each errors as e}<li><span class="muted small">{when(e.at)}</span> {e.status} · {e.method} {e.path}</li>{:else}<li class="muted">Aucune</li>{/each}
    </ul>
  {/if}
</div>

<style>
  .adm-page { padding: var(--espace-4); overflow: auto; flex: 1; min-height: 0; }
  h3 { margin: 0.75rem 0 0.25rem; }
  .row { flex-wrap: wrap; gap: 0.5rem; margin: 0.25rem 0; }
  .adm { margin: 0; padding-left: 1.25rem; overflow-wrap: anywhere; }
  .adm > li { margin: 0.35rem 0; }
  textarea { resize: vertical; }
</style>
