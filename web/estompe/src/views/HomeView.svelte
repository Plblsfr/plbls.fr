<script lang="ts">
  import Avatar from '../components/Avatar.svelte'
  import Icon from '../components/Icon.svelte'
  import { app } from '../lib/store.svelte'
  import { voice } from '../blocks/vocal/voice.svelte'
  import { hrefs, router } from '../lib/route.svelte'
  import { openSearch } from '../lib/search.svelte'
  import { STATUS_LABEL, timeOf } from '../lib/format'
  import { convKeyOf, type ConvKey, type Mention } from '../lib/types'

  let { onmenu, oncreate }: { onmenu: () => void; oncreate: () => void } = $props()

  let now = $state(Date.now())
  $effect(() => {
    const t = setInterval(() => (now = Date.now()), 30_000)
    return () => clearInterval(t)
  })

  const jour = new Intl.DateTimeFormat('fr-FR', { weekday: 'long', day: 'numeric', month: 'long' })
  const courte = new Intl.DateTimeFormat('fr-FR', { day: '2-digit', month: '2-digit' })
  const date = $derived(jour.format(now))
  const salut = $derived.by(() => {
    const h = new Date(now).getHours()
    return h >= 18 || h < 5 ? 'Bonsoir' : 'Bonjour'
  })

  const quand = (iso: string) => new Date(iso).toDateString() === new Date(now).toDateString() ? timeOf(iso) : courte.format(new Date(iso))

  function lieu(key: ConvKey): { nom: string; href: string; dm: boolean } | null {
    if (key.startsWith('d:')) {
      const d = app.dms.find((x) => x.id === key.slice(2))
      return d ? { nom: d.other.pseudo, href: hrefs.dm(d.id), dm: true } : null
    }
    const c = app.channel(key.slice(2))
    return c ? { nom: `${c.server.name} › # ${c.channel.name}`, href: hrefs.channel(c.server.id, c.channel.id), dm: false } : null
  }

  const mentions = $derived.by(() => {
    const ignored = app.notifySettings.ignore_group ?? []
    return app.mentions
      .filter((m) => !(m.group && m.server_id && ignored.includes(m.server_id)))
      .map((m) => ({ m, l: lieu(convKeyOf(m)) }))
      .filter((x): x is { m: Mention; l: NonNullable<ReturnType<typeof lieu>> } => x.l !== null)
      .sort((a, b) => b.m.created_at.localeCompare(a.m.created_at))
  })

  const nonLus = $derived.by(() => {
    const avecMention = new Set(mentions.map((x) => convKeyOf(x.m)))
    return (Object.entries(app.unread) as [ConvKey, number][])
      .filter(([k, n]) => n > 0 && !avecMention.has(k) && k !== app.current)
      .map(([k, n]) => ({ k, n, l: lieu(k) }))
      .filter((x): x is { k: ConvKey; n: number; l: NonNullable<ReturnType<typeof lieu>> } => x.l !== null)
  })

  function toutLu() {
    for (const k of new Set([...mentions.map((x) => convKeyOf(x.m)), ...nonLus.map((x) => x.k)])) app.readConv(k)
  }

  const vocaux = $derived(
    Object.entries(app.voice)
      .filter(([, ps]) => ps.length > 0)
      .map(([id, ps]) => ({ id, ps, info: app.channel(id) }))
      .filter((v): v is typeof v & { info: NonNullable<typeof v.info> } => v.info !== null),
  )

  function depuis(ps: { since?: string }[]): string {
    const t = Math.min(...ps.map((p) => Date.parse(p.since ?? '')).filter((x) => !Number.isNaN(x)))
    if (!Number.isFinite(t)) return ''
    const min = Math.max(0, Math.round((now - t) / 60_000))
    return min < 60 ? `${min} min` : `${Math.floor(min / 60)} h ${String(min % 60).padStart(2, '0')}`
  }

  const dedans = (id: string) => voice.channelId === id && voice.state !== 'idle'

  function rejoindre(serverId: string, channelId: string) {
    router.go(hrefs.channel(serverId, channelId))
    if (!dedans(channelId) && app.can(serverId, 'connect', channelId)) void voice.join(channelId)
  }

  const enLigne = $derived(app.dms.filter((d) => app.presenceOf(d.other.id) !== 'off'))

  function occupation(id: string): string {
    for (const v of vocaux) if (v.ps.some((p) => p.user_id === id)) return `En vocal · ${v.info.channel.name}`
    return app.activityOf(id) || app.listeningOf(id) || STATUS_LABEL[app.presenceOf(id)]
  }

  const resume = $derived.by(() => {
    const n = mentions.length
    const parts = [
      n ? `${n} mention${n > 1 ? 's' : ''}` : '',
      nonLus.length ? `${nonLus.length} conversation${nonLus.length > 1 ? 's' : ''} non lue${nonLus.length > 1 ? 's' : ''}` : '',
      vocaux.length ? `${vocaux.length} salon${vocaux.length > 1 ? 's' : ''} vocal${vocaux.length > 1 ? 'aux' : ''} actif${vocaux.length > 1 ? 's' : ''}` : '',
    ].filter(Boolean)
    return parts.length ? `${parts.join(', ')}. Voici ce qui t'attend.` : 'Rien de neuf pour l\'instant.'
  })

</script>

<header class="main-head">
  <button class="ib burger" onclick={onmenu} aria-label="Menu"><Icon name="menu" /></button>
  <h1 class="grow">Accueil</h1>
  <button class="v-btn sec sm palette" onclick={openSearch} aria-label="Rechercher ou aller à…" aria-keyshortcuts="Control+K">
    <Icon name="cherche" size={16} /><span class="palette-libelle">Rechercher ou aller à…</span><span class="v-kbd">Ctrl K</span>
  </button>
</header>

<div class="accueil">
  <div class="contenu">
    <div class="salut">
      <span class="v-eyebrow">{date}</span>
      {#if app.servers.length === 0}
        <h2 class="v-titre">Bienvenue, <span class="v-surl">{app.pseudo}</span>.</h2>
      {:else}
        <h2 class="v-titre">{salut}, <span class="v-surl">{app.pseudo}</span>.</h2>
        <p class="v-mute">{resume}</p>
      {/if}
    </div>

    {#if app.servers.length === 0}
      <div class="v-empty depart">
        <span class="depart-icone" aria-hidden="true"><Icon name="calque" size={32} /></span>
        <div class="depart-texte">
          <b>Pour commencer</b>
          <p class="v-note">Tu n'es dans aucun serveur. Crée le tien, ou rejoins celui d'un ami avec son lien ou son code d'invitation.</p>
        </div>
        <button class="plbls-button" onclick={oncreate}>Créer ou rejoindre un serveur</button>
      </div>
    {/if}

    {#if mentions.length || nonLus.length || vocaux.length}
      <div class="grille">
        {#if mentions.length || nonLus.length}
          <section class="v-card carte" aria-labelledby="h-rattraper">
            <div class="carte-tete"><h3 id="h-rattraper" class="grow">À rattraper</h3><button class="v-btn sm ghost" onclick={toutLu}>Tout marquer lu</button></div>
            <ul class="lignes">
              {#each mentions.slice(0, 6) as { m, l } (m.message_id)}
                <li>
                  <button class="ligne mention" onclick={() => router.go(`${l.href}/${m.message_id}`)}>
                    <Avatar pseudo={m.author.pseudo} userId={m.author.id} serverId={m.server_id ?? ''} size="sm" />
                    <span class="ligne-corps">
                      <span class="ligne-meta"><span class="v-ell"><b>{m.author.pseudo}</b> {l.dm ? 'en message privé' : 'dans'} {#if !l.dm}<b>{l.nom}</b>{/if}</span><span class="v-mono v-mute2">{quand(m.created_at)}</span></span>
                      <span class="v-ell">{m.content || 'Pièce jointe'}</span>
                    </span>
                  </button>
                </li>
              {/each}
              {#each nonLus.slice(0, 6) as { k, n, l } (k)}
                <li>
                  <button class="ligne" onclick={() => router.go(l.href)}>
                    <span class="ligne-icone" aria-hidden="true"><Icon name={l.dm ? 'arobase' : 'hash'} size={16} /></span>
                    <span class="ligne-corps">
                      <span class="v-ell"><b>{l.nom}</b></span>
                      <span class="v-note">{n} nouveau{n > 1 ? 'x' : ''} message{n > 1 ? 's' : ''}</span>
                    </span>
                    <span class="v-badge rose">{n}</span>
                  </button>
                </li>
              {/each}
            </ul>
          </section>
        {/if}

        {#if vocaux.length}
          <section class="v-card carte" aria-labelledby="h-moment">
            <div class="carte-tete"><h3 id="h-moment" class="grow">En ce moment</h3><span class="v-tag ok"><Icon name="onde" size={12} />{vocaux.reduce((n, v) => n + v.ps.length, 0)} en vocal</span></div>
            {#each vocaux as v (v.id)}
              {@const musique = app.music[v.id]?.state.meta}
              <div class="vocal">
                <span class="vocal-icone" aria-hidden="true"><Icon name={v.info.channel.kind === 'cinema' ? 'film' : 'vol'} size={20} /></span>
                <span class="ligne-corps">
                  <b class="v-ell">{v.info.channel.name}</b>
                  <span class="v-note v-ell">{v.info.server.name}{depuis(v.ps) ? ` · ${depuis(v.ps)}` : ''}{musique ? ` · ${musique.title}` : ''}</span>
                </span>
                <span class="vocal-gens">
                  {#each v.ps.slice(0, 4) as p (p.user_id)}<Avatar pseudo={p.pseudo} userId={p.user_id} serverId={v.info.server.id} size="sm" speaking={voice.channelId === v.id && !!voice.speaking[p.user_id]} />{/each}
                  {#if v.ps.length > 4}<span class="v-note">+{v.ps.length - 4}</span>{/if}
                </span>
              </div>
              <button class="v-btn blk {dedans(v.id) ? 'sec' : 'rose'}" onclick={() => rejoindre(v.info.server.id, v.id)}>
                {dedans(v.id) ? `Ouvrir « ${v.info.channel.name} »` : app.can(v.info.server.id, 'connect', v.id) ? `Rejoindre « ${v.info.channel.name} »` : `Voir « ${v.info.channel.name} »`}
              </button>
            {/each}
          </section>
        {/if}
      </div>
    {/if}

    {#if enLigne.length}
      <section class="v-card carte" aria-labelledby="h-en-ligne">
        <div class="carte-tete"><h3 id="h-en-ligne" class="grow">En ligne</h3><span class="v-note">{enLigne.length} sur {app.dms.length}</span></div>
        <ul class="gens">
          {#each enLigne as d (d.id)}
            <li>
              <button class="personne" onclick={() => router.go(hrefs.dm(d.id))}>
                <Avatar pseudo={d.other.pseudo} userId={d.other.id} status={app.presenceOf(d.other.id)} />
                <span class="ligne-corps"><b class="v-ell">{d.other.pseudo}</b><span class="v-note v-ell">{occupation(d.other.id)}</span></span>
              </button>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if app.servers.length > 0}
      <div class="v-empty">
        <Icon name="bulle" size={28} />
        <b>Choisis une conversation</b>
        <p class="v-note">Sélectionne un serveur à gauche, ou ouvre un message privé.</p>
      </div>
    {/if}
  </div>
</div>

<style>
  .palette { gap: 10px; color: var(--gris); }
  .palette .v-kbd { margin-left: 4px; }
  .accueil { flex: 1; min-height: 0; overflow: auto; }
  .contenu { display: flex; flex-direction: column; gap: 28px; max-width: 1100px; padding: 32px 40px 48px; }
  .salut { display: flex; flex-direction: column; gap: 6px; }
  .salut h2 { font-size: clamp(32px, 5vw, 44px); }
  .salut p { margin: 0; }
  .depart { flex-direction: row; flex-wrap: wrap; gap: 20px; padding: 20px 24px; text-align: left; }
  .depart-icone { color: var(--rose-texte); }
  .depart-texte { flex: 1 1 260px; display: flex; flex-direction: column; gap: 2px; }
  .depart-texte p, .v-empty p { margin: 0; }
  .grille { display: grid; grid-template-columns: repeat(auto-fit, minmax(min(100%, 340px), 1fr)); gap: 20px; align-items: start; }
  .carte { display: flex; flex-direction: column; gap: 12px; padding: 18px; }
  .carte-tete { display: flex; align-items: center; gap: 8px; }
  .carte-tete h3 { margin: 0; font-size: 15px; font-weight: 700; }
  .lignes, .gens { margin: 0; padding: 0; list-style: none; }
  .lignes { display: flex; flex-direction: column; gap: 2px; margin: 0 -6px; }
  .ligne, .personne { display: flex; align-items: center; gap: 12px; width: 100%; min-width: 0; padding: 10px 8px; border: 0; border-radius: var(--r-s); background: transparent; color: var(--encre); font: inherit; text-align: left; cursor: pointer; transition: background var(--d) var(--sortie); }
  .ligne:hover, .personne:hover { background: var(--papier-2); }
  .ligne.mention { background: var(--rose-doux); }
  .ligne.mention:hover { background: color-mix(in srgb, var(--rose) 60%, var(--rose-doux)); }
  .ligne:focus-visible, .personne:focus-visible { outline: none; box-shadow: var(--anneau); }
  .ligne-corps { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; font-size: 14px; }
  .ligne-meta { display: flex; align-items: baseline; gap: 8px; color: var(--gris); font-size: 13px; }
  .ligne-meta > .v-ell { flex: 1; }
  .ligne-meta b { color: var(--encre); }
  .ligne-icone { display: inline-grid; place-items: center; flex: none; width: 24px; height: 24px; border-radius: 8px; background: var(--papier-2); color: var(--gris); }
  .vocal { display: flex; align-items: center; gap: 12px; min-width: 0; }
  .vocal-icone { display: inline-grid; place-items: center; flex: none; width: 44px; height: 44px; border-radius: 14px; background: var(--rose); color: var(--encre); }
  .vocal-gens { display: flex; align-items: center; gap: 4px; flex: none; }
  .gens { display: grid; grid-template-columns: repeat(auto-fill, minmax(min(100%, 200px), 1fr)); gap: 4px; margin: 0 -6px; }
  @media (max-width: 900px) {
    .contenu { padding: 24px 16px 40px; gap: 20px; }
  }
  @media (max-width: 560px) {
    .palette-libelle, .palette .v-kbd { display: none; }
    .palette { padding: 0 10px; }
  }
  @media (pointer: coarse) {
    .ligne, .personne { min-height: 44px; }
    .palette { min-height: 44px; min-width: 44px; }
  }
</style>
