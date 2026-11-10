<script lang="ts">
  import { confirmer } from '../lib/confirm.svelte'
  import { onMount } from 'svelte'
  import BadgeAdmin from '../blocks/badges/BadgeAdmin.svelte'
  import AdminReports from '../blocks/signalements/AdminReports.svelte'
  import Icon from './Icon.svelte'
  import ServerIcon from './ServerIcon.svelte'
  import { api } from '../lib/api'
  import { adminSection } from '../lib/admin'
  import { app } from '../lib/store.svelte'
  import PermissionsAdmin from './permissions/PermissionsAdmin.svelte'
  import AccesAdmin from './acces/AccesAdmin.svelte'
  import { hrefs } from '../lib/route.svelte'
  import type { AdminBadge, AdminError, AdminLive, AdminLogEntry, AdminOverview, AdminStats, AdminUser } from '../lib/types'

  let { section, onmenu }: { section?: string; onmenu: () => void } = $props()
  const current = $derived(adminSection(section, app.adminSections))
  const label = $derived(app.adminSections.find((s) => s.id === current)?.label ?? '')
  const title = $derived(label.charAt(0).toUpperCase() + label.slice(1))

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

  const fail = (e: unknown) => (error = String((e as Error).message ?? e))
  const load = (f: () => Promise<unknown>) => f().then(() => (error = '')).catch(fail)
  const act = (f: () => Promise<unknown>, reload: () => Promise<unknown>) => load(async () => { await f(); await reload() })
  const when = (d: string) => new Date(d).toLocaleString('fr-FR')
  const yes = (b: boolean) => (b ? 'configuré' : 'non configuré')
  const mb = (n: number) => `${Math.round(n / (1 << 20))} Mo`
  const num = (n: number) => n.toLocaleString('fr-FR')
  const usage = $derived(stats && ov?.config.settings.max_storage ? Math.min(100, (stats.storage_bytes / ov.config.settings.max_storage) * 100) : null)
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
  <h1 class="grow"><span class="v-mute adm-fil">Estompe ›</span> {title}</h1>
  {#if current === 'stats'}<span class="v-note adm-live"><span class="adm-pt" aria-hidden="true"></span>actualisé toutes les 5 s</span>{/if}
</header>
<div class="adm-page">
  {#if error}<div class="v-alert dgr" role="alert"><Icon name="alerte" size={18} /><span>{error}</span></div>{/if}
  {#if info}<div class="v-alert ok" role="status"><Icon name="ok" size={18} /><span>{info}</span></div>{/if}

  {#if current === 'users'}
    <section class="v-card adm-card">
      <div class="adm-h">
        <h2 class="adm-t grow">Utilisateurs</h2>
        <form class="adm-search" role="search" onsubmit={(e) => { e.preventDefault(); void load(loadUsers) }}>
          <input class="plbls-input" type="search" placeholder="Rechercher un pseudo" aria-label="Rechercher un pseudo" bind:value={query} />
          <button class="plbls-button plbls-button--compact">Rechercher</button>
        </form>
      </div>
      <div class="adm-scroll">
        <table class="adm-tb">
          <thead><tr><th scope="col">Pseudo</th><th scope="col">Inscrit</th><th scope="col">Activité</th><th scope="col"><span class="v-ell adm-sr">Actions</span></th></tr></thead>
          <tbody>
            {#each users as u (u.id)}
              <tr class:off={u.suspended}>
                <td><b>{u.pseudo}</b>{#if u.suspended} <span class="v-tag dgr"><Icon name="interdit" size={12} />suspendu</span>{/if}</td>
                <td class="v-mono">inscrit le {new Date(u.created_at).toLocaleDateString('fr-FR')}</td>
                <td class="v-note">{u.servers} serveur(s) · {num(u.messages)} message(s)</td>
                <td class="adm-act"><button class="plbls-button plbls-button--compact" class:plbls-button--contour={!u.suspended} onclick={() => suspend(u)}>{u.suspended ? 'Rétablir' : 'Suspendre'}</button></td>
              </tr>
            {:else}<tr><td colspan="4" class="v-note">Aucun résultat</td></tr>{/each}
          </tbody>
        </table>
      </div>
    </section>

  {:else if current === 'servers'}
    <section class="v-card adm-card">
      <h2 class="adm-t">Serveurs</h2>
      <div class="adm-scroll">
        <table class="adm-tb">
          <thead><tr><th scope="col">Serveur</th><th scope="col">Membres</th><th scope="col">Messages</th><th scope="col">Dernier message</th><th scope="col"><span class="adm-sr">Actions</span></th></tr></thead>
          <tbody>
            {#each ov?.servers ?? [] as s (s.id)}
              <tr>
                <td><span class="adm-srv"><ServerIcon server={{ id: s.id, name: s.name, icon_version: 0 }} /><b>{s.name}</b></span></td>
                <td class="v-mono">{s.members} membres · {s.channels} salons</td>
                <td class="v-mono">{num(s.messages)} messages</td>
                <td class="v-mono v-mute">{s.last_message_at ? `dernier ${when(s.last_message_at)}` : '—'}</td>
                <td class="adm-act"><button class="plbls-button plbls-button--compact plbls-button--discret adm-dgr" onclick={() => dropServer(s.id, s.name)}>Supprimer</button></td>
              </tr>
            {:else}<tr><td colspan="5" class="v-note">Aucun serveur</td></tr>{/each}
          </tbody>
        </table>
      </div>
    </section>

  {:else if current === 'badges'}
    <BadgeAdmin {badges} reload={loadBadges} {fail} />

  {:else if current === 'reports'}
    <AdminReports {fail} />

  {:else if current === 'bot'}
    <section class="v-card adm-card adm-narrow">
      <h2 class="adm-t">Bot « Estompe »</h2>
      <p class="v-note">Messages privés envoyés par le compte « Estompe » (lecture seule pour les utilisateurs).</p>
      <form class="adm-form" onsubmit={(e) => { e.preventDefault(); sendBot() }}>
        <div class="plbls-field">
          <label class="plbls-field__label" for="adm-to">Destinataire</label>
          <input id="adm-to" class="plbls-input" placeholder="Pseudo du destinataire (vide : tous les comptes)" bind:value={bot.to} maxlength="20" />
        </div>
        <div class="plbls-field">
          <label class="plbls-field__label" for="adm-msg">Message</label>
          <textarea id="adm-msg" class="plbls-input plbls-textarea" rows="4" placeholder="Message" bind:value={bot.content} required maxlength="4000"></textarea>
        </div>
        <div class="adm-f"><span class="v-note grow">Confirmation avant envoi</span><button class="plbls-button"><Icon name="envoi" size={16} />Envoyer</button></div>
      </form>
      <hr class="v-hr" />
      <h3 class="v-eyebrow">Notes de version envoyées</h3>
      <ul class="adm-list">{#each bot.releases as v}<li class="v-mono">{v}</li>{:else}<li class="v-note">Aucune</li>{/each}</ul>
    </section>

  {:else if current === 'stats'}
    {#if stats}
      <div class="adm-stats">
        <div class="adm-st"><span class="v-eyebrow">Comptes</span><b>{num(stats.users)}</b><span class="v-note">{stats.new_users_7d} cette semaine · {stats.suspended} suspendu(s)</span></div>
        <div class="adm-st"><span class="v-eyebrow">Serveurs · messages</span><b>{num(stats.servers)} · {num(stats.messages)}</b><span class="v-note">+{num(stats.messages_24h)} ces dernières 24 h</span></div>
        <div class="adm-st"><span class="v-eyebrow">Stockage</span><b>{mb(stats.storage_bytes)}</b>
          {#if usage !== null}<div class="v-prog" role="progressbar" aria-label="Stockage utilisé" aria-valuenow={Math.round(usage)} aria-valuemin={0} aria-valuemax={100}><i style:width="{usage}%"></i></div>{/if}
          <span class="v-note">{#if ov}sur {Math.round(ov.config.settings.max_storage / (1 << 30))} Go · {/if}{num(stats.files)} fichier(s)</span></div>
        <div class="adm-st" class:dgr={stats.errors_24h > 0}><span class="v-eyebrow">Erreurs 24 h</span><b>{stats.errors_24h}</b><a class="v-note" href={hrefs.admin('logs')}>Voir les logs</a></div>
      </div>
    {/if}
    <div class="adm-grid">
      <section class="v-card adm-card">
        <div class="adm-h"><h2 class="adm-t grow">En direct</h2>{#if live}<span class="v-mono v-mute">{live.connections} connexion(s) temps réel</span>{/if}</div>
        {#if live}
          <p class="v-note">{live.online.length} appareil(s) connecté(s)</p>
          <ul class="adm-kv">
            {#each live.online as o}<li><span class="grow"><b>{o.pseudo}</b></span><span class="v-tag">{o.platform || '?'}</span><span class="v-mono v-mute">{o.version}</span></li>{:else}<li class="v-note">Personne</li>{/each}
          </ul>
          <h3 class="adm-t2">Salons vocaux actifs</h3>
          <ul class="adm-kv">
            {#each live.voice as v}
              <li class="adm-voc">
                <span class="adm-voc-h"><Icon name="vol" size={16} /><span class="grow">{v.server} › {v.channel}</span><span class="v-mono">{v.participants.length}</span></span>
                <ul class="adm-parts">{#each v.participants as p}<li>{p.pseudo}{#if p.screen} <span class="v-tag"><Icon name="ecran" size={12} />écran</span>{/if}{#if p.camera} <span class="v-tag"><Icon name="cam" size={12} />caméra</span>{/if}</li>{/each}</ul>
              </li>
            {:else}<li class="v-note">Aucun</li>{/each}
          </ul>
        {:else}<p class="v-note">Chargement…</p>{/if}
      </section>
      {#if ov}
        <div class="adm-col">
          <section class="v-card adm-card">
            <h2 class="adm-t">Configuration</h2>
            <ul class="adm-kv">
              <li><span class="grow">Notifications push (web)</span><span class="v-tag" class:ok={ov.config.settings.push_web}>{yes(ov.config.settings.push_web)}</span></li>
              <li><span class="grow">Notifications push (Android)</span><span class="v-tag" class:ok={ov.config.settings.push_android}>{yes(ov.config.settings.push_android)}</span></li>
              <li><span class="grow">GIPHY</span><span class="v-tag" class:ok={ov.config.settings.giphy}>{yes(ov.config.settings.giphy)}</span></li>
              <li><span class="grow">Métriques</span><span class="v-tag" class:ok={ov.config.settings.metrics}>{ov.config.settings.metrics ? 'activées' : 'désactivées'}</span></li>
              <li class="adm-wrap"><span class="grow">LiveKit</span><span class="v-mono adm-val">{ov.config.settings.livekit_url} · {ov.config.max_participants} participants max · caméra et écran {ov.config.settings.allow_video ? 'autorisés' : 'désactivés'}{ov.config.screen_max_fps ? ` · écran ${ov.config.screen_max_height}p, ${ov.config.screen_max_fps} i/s` : ''}</span></li>
              <li><span class="grow">Stockage</span><span class="v-mono">{Math.round(ov.config.settings.max_storage / (1 << 30))} Go · fichiers {mb(ov.config.max_file_size)} max</span></li>
              <li class="adm-wrap"><span class="grow">Origines autorisées</span><span class="v-mono adm-val">{(ov.config.settings.origins ?? []).join(', ') || '—'}</span></li>
            </ul>
          </section>
          <section class="v-card adm-card">
            <h2 class="adm-t">Versions</h2>
            <ul class="adm-kv">
              <li><span class="grow">Backend</span><span class="v-mono">{ov.version}</span></li>
              <li><span class="grow">Front</span><span class="v-mono">{__APP_VERSION__}</span></li>
              <li><span class="grow">Installeur Windows</span><span class="v-mono">{installers.windows}</span></li>
              <li><span class="grow">Application Android</span><span class="v-mono">{installers.android}</span></li>
            </ul>
          </section>
        </div>
      {/if}
    </div>

  {:else if current === 'logs'}
    <section class="v-card adm-card">
      <h2 class="adm-t">Actions d'administration</h2>
      <div class="adm-scroll">
        <table class="adm-tb">
          <thead><tr><th scope="col">Date</th><th scope="col">Par</th><th scope="col">Action</th><th scope="col">Cible</th></tr></thead>
          <tbody>
            {#each log as l}<tr><td class="v-mono v-mute">{when(l.at)}</td><td><b>{l.actor}</b></td><td>{l.action}</td><td class="v-note">{l.target || '—'}</td></tr>{:else}<tr><td colspan="4" class="v-note">Aucune</td></tr>{/each}
          </tbody>
        </table>
      </div>
    </section>
    <section class="v-card adm-card">
      <h2 class="adm-t">Erreurs serveur (30 jours)</h2>
      {#if errors.length}
        <pre class="adm-pre">{#each errors as e}<span class="v-mute">{when(e.at)}</span>  <b>{e.status}</b>  {e.method} {e.path}{'\n'}{/each}</pre>
      {:else}<p class="v-note">Aucune</p>{/if}
    </section>

  {:else if current === 'acces'}
    <AccesAdmin />
  {:else if current === 'permissions'}
    <PermissionsAdmin />
  {/if}
</div>

<style>
  .main-head h1 { font-family: var(--f-titre); font-size: 20px; letter-spacing: -0.02em; }
  .adm-fil { font-family: var(--f-texte); font-size: 14px; font-weight: 500; }
  .adm-live { display: inline-flex; align-items: center; gap: 6px; padding-right: 8px; }
  .adm-pt { width: 8px; height: 8px; border-radius: 50%; background: var(--ok); }
  .adm-page { flex: 1; min-height: 0; overflow: auto; display: flex; flex-direction: column; gap: 20px; padding: 24px; background: var(--papier); scrollbar-width: thin; }
  .adm-card { display: flex; flex-direction: column; gap: 12px; min-width: 0; }
  .adm-narrow { max-width: 720px; }
  .adm-h { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
  .adm-t { margin: 0; font-family: var(--f-titre); font-size: 20px; font-weight: 700; letter-spacing: -0.02em; }
  .adm-t2 { margin: 10px 0 0; font-family: var(--f-titre); font-size: 17px; font-weight: 700; }
  .adm-search { display: flex; gap: 8px; flex-wrap: wrap; }
  .adm-search input { width: 240px; max-width: 100%; height: 36px; }
  .adm-scroll { overflow-x: auto; }
  .adm-tb { width: 100%; border-collapse: collapse; font-size: 13.5px; }
  .adm-tb th { padding: 8px 10px; border-bottom: 1px solid var(--trait); color: var(--gris); font-family: var(--f-mono); font-size: 10.5px; font-weight: 600; letter-spacing: 0.08em; text-align: left; text-transform: uppercase; white-space: nowrap; }
  .adm-tb td { padding: 10px; border-bottom: 1px solid var(--trait); vertical-align: middle; }
  .adm-tb tbody tr:last-child td { border-bottom: 0; }
  .adm-tb tr.off td { background: var(--danger-fond); }
  .adm-act { text-align: right; white-space: nowrap; }
  .adm-sr { position: absolute; width: 1px; height: 1px; overflow: hidden; clip-path: inset(50%); white-space: nowrap; }
  .adm-dgr { color: var(--danger); }
  .adm-srv { display: inline-flex; align-items: center; gap: 8px; }
  .adm-srv :global(.sv-icon) { width: 28px; height: 28px; border-radius: 9px; font-size: 10px; }
  .adm-form { display: flex; flex-direction: column; gap: 12px; }
  .adm-f { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  textarea { resize: vertical; }
  .adm-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 6px; }
  h3.v-eyebrow { margin: 0; font-weight: 400; }
  .adm-stats { display: grid; grid-template-columns: repeat(auto-fit, minmax(200px, 1fr)); gap: 14px; }
  .adm-st { display: flex; flex-direction: column; gap: 4px; padding: 18px; border-radius: var(--r-l); background: var(--surface); box-shadow: var(--o1); min-width: 0; }
  .adm-st b { font-family: var(--f-titre); font-size: 34px; letter-spacing: -0.04em; line-height: 1.1; overflow-wrap: anywhere; }
  .adm-st .v-prog { margin-top: 6px; }
  .adm-st.dgr { box-shadow: inset 0 0 0 1.5px var(--danger); }
  .adm-st.dgr .v-eyebrow, .adm-st.dgr b { color: var(--danger); }
  .adm-st a { color: var(--encre); width: fit-content; }
  .adm-grid { display: grid; grid-template-columns: minmax(0, 1.2fr) minmax(0, 1fr); gap: 20px; align-items: start; }
  .adm-col { display: flex; flex-direction: column; gap: 20px; min-width: 0; }
  .adm-kv { list-style: none; margin: 0; padding: 0; }
  .adm-kv > li { display: flex; align-items: center; gap: 10px; padding: 8px 0; border-bottom: 1px solid var(--trait); font-size: 13.5px; }
  .adm-kv > li:last-child { border-bottom: 0; }
  .adm-kv > li > .grow { color: var(--encre-2); }
  .adm-wrap { flex-wrap: wrap; }
  .adm-val { overflow-wrap: anywhere; text-align: right; }
  .adm-voc { flex-direction: column; align-items: stretch; }
  .adm-voc-h { display: flex; align-items: center; gap: 8px; }
  .adm-parts { margin: 4px 0 0; padding-left: 24px; display: flex; flex-direction: column; gap: 2px; color: var(--encre-2); }
  .adm-pre { margin: 0; padding: 12px; overflow: auto; max-height: 24rem; border-radius: var(--r); background: var(--encre); color: var(--papier); font-family: var(--f-mono); font-size: 11.5px; line-height: 1.6; white-space: pre-wrap; overflow-wrap: anywhere; }
  .adm-pre .v-mute { color: color-mix(in srgb, var(--papier) 65%, transparent); }
  @media (max-width: 960px) { .adm-grid { grid-template-columns: minmax(0, 1fr); } }
  @media (max-width: 720px) { .adm-page { padding: 16px; } .adm-search input { width: 100%; } .adm-search { flex: 1 1 100%; } }
</style>
