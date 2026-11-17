<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import { confirmer } from '../../lib/confirm.svelte'
  import { notify } from '../../lib/ui'
  import SillonAdditions from './SillonAdditions.svelte'
  import SillonCover from './SillonCover.svelte'
  import { echec } from './sillon.svelte'
  import { ETATS_AJOUT, octets, occupation, refusAjout, sillonApi, type Ajout, type Album, type Analyse, type Decouverte, type Disque, type Statut } from './sillon'

  type Onglet = 'disque' | 'albums' | 'synchro' | 'analyse' | 'ecartes' | 'decouverte'
  const ONGLETS: { id: Onglet; label: string }[] = [
    { id: 'disque', label: 'Disque et ajouts' }, { id: 'albums', label: 'Albums' }, { id: 'synchro', label: 'Synchronisation' },
    { id: 'analyse', label: 'Analyse' }, { id: 'ecartes', label: 'Fichiers écartés' }, { id: 'decouverte', label: 'Découverte' },
  ]
  const RAISONS: Record<string, string> = {
    hors_medias: 'Hors du dossier de la musique', extension_interdite: 'Extension interdite', double_extension: 'Double extension', conteneur_incoherent: 'Conteneur incohérent',
    sans_flux_audio: 'Sans flux audio', nom_suspect: 'Nom suspect', analyse_impossible: 'Analyse impossible',
  }

  let onglet = $state<Onglet>('disque')
  let disque = $state<Disque | null>(null)
  let albums = $state<(Album & { size: number; added_by: string | null })[] | null>(null)
  let jamais = $state(false)
  let statut = $state<Statut | null>(null)
  let analyse = $state<Analyse | null>(null)
  let ecartes = $state<{ name: string; reason: string; seen_at: string }[] | null>(null)
  let decouverte = $state<Decouverte | null>(null)
  let liste = $state<{ charger: () => Promise<void> }>()
  const date = (s: string | null) => (s ? new Date(s).toLocaleString('fr-FR', { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' }) : '—')
  const part = (d: Disque) => (d.limit ? Math.min(100, ((d.used + d.pending) / d.limit) * 100) : 0)

  const chargeurs: Record<Onglet, () => Promise<void>> = {
    disque: async () => { disque = await sillonApi.disque() },
    albums: async () => { albums = (await sillonApi.albumsAdmin(jamais)).items },
    synchro: async () => { statut = await sillonApi.statut() },
    analyse: async () => { analyse = await sillonApi.analyse() },
    ecartes: async () => { ecartes = (await sillonApi.rejetes()).items },
    decouverte: async () => { decouverte = await sillonApi.decouverte() },
  }
  const charger = (o: Onglet) => chargeurs[o]().catch((e) => echec(e, 'L’administration est indisponible'))

  $effect(() => { void charger(onglet) })

  async function relancer(a: Ajout) {
    try {
      await sillonApi.relancerAjout(a.id)
      notify.ok(`« ${a.title} » est relancé.`)
      await Promise.all([liste?.charger(), charger('disque')])
    } catch (e) { notify.fail(refusAjout(e)) }
  }

  async function supprimer(a: Album, artiste: boolean, apres: Onglet = 'albums') {
    const nom = artiste ? a.artist.name : a.title
    const ok = await confirmer({
      titre: artiste ? `Supprimer l’artiste « ${nom} » ?` : `Supprimer l’album « ${nom} » ?`,
      message: `${artiste ? 'Tous ses albums et leurs fichiers' : 'L’album et ses fichiers'} seront supprimés dans Lidarr et ne reviendront pas seuls. Ses titres restent dans les playlists, indisponibles.`,
      action: 'Supprimer', destructif: true,
    })
    if (!ok) return
    try {
      await (artiste ? sillonApi.supprimerArtiste(a.artist.id) : sillonApi.supprimerAlbum(a.id))
      notify.ok(`« ${nom} » est supprimé.`)
      await charger(apres)
    } catch (e) { notify.error(e, 'La suppression a échoué') }
  }

  async function synchroniser() {
    try { await sillonApi.synchroniser(); notify.ok('Synchronisation lancée.'); await charger('synchro') } catch (e) { notify.error(e, 'La synchronisation n’a pas pu être lancée') }
  }

  async function relancerAnalyse() {
    try { const r = await sillonApi.relancerAnalyse(); notify.ok(`${r.requeued} analyse${r.requeued > 1 ? 's' : ''} remise${r.requeued > 1 ? 's' : ''} en file.`); await charger('analyse') } catch (e) { notify.error(e, 'L’analyse n’a pas pu être relancée') }
  }
</script>

<div class="film-page">
  <div class="film-filters"><h2 class="v-titre film-filters-title">Administration</h2></div>
  <nav class="v-tabs sillon-onglets" aria-label="Administration de Sillon">
    {#each ONGLETS as o (o.id)}<button aria-selected={onglet === o.id} role="tab" onclick={() => (onglet = o.id)}>{o.label}</button>{/each}
  </nav>

  {#if onglet === 'disque'}
    <section class="film-section">
      {#if disque}
        <div class="v-card sillon-disque-carte">
          <p class="v-eyebrow">Occupation du dossier de la musique</p>
          <p class="sillon-chiffre">{occupation(disque)}</p>
          {#if disque.limit}<div class="v-prog rose" role="progressbar" aria-label="Occupation" aria-valuenow={Math.round(part(disque))} aria-valuemin="0" aria-valuemax="100"><i style="width:{part(disque)}%"></i></div>{/if}
          <p class="v-note">{octets(disque.used)} présents · {octets(disque.pending)} en cours de téléchargement{disque.free != null ? ` · ${octets(disque.free)} libres` : ''} · {disque.daily_limit} ajouts par personne et par jour{disque.configured ? '' : ' · ajout non configuré'}</p>
        </div>
      {:else}<Loader />{/if}
    </section>
    <section class="film-section">
      <h2 class="v-titre">Ajouts</h2>
      <SillonAdditions admin {relancer} bind:this={liste} />
    </section>
  {:else if onglet === 'albums'}
    <section class="film-section">
      <label class="sillon-filtre"><input type="checkbox" bind:checked={jamais} onchange={() => void charger('albums')} /> Seulement les albums jamais écoutés</label>
      {#if !albums}
        <Loader />
      {:else if !albums.length}
        <div class="v-empty"><Icon name="note" size={28} /><p class="v-note">Aucun album.</p></div>
      {:else}
        <ul class="sillon-admin-liste">
          {#each albums as a (a.id)}
            <li>
              <span class="sillon-admin-art"><SillonCover id={a.id} title={a.title} image={a.cover} size="small" /></span>
              <span class="grow sillon-admin-t"><strong class="truncate">{a.title}</strong><span class="v-note truncate">{[a.artist.name, octets(a.size), a.added_by ? `ajouté par ${a.added_by}` : ''].filter(Boolean).join(' · ')}</span></span>
              <button class="v-btn sm sec" onclick={() => void supprimer(a, false)} aria-label={`Supprimer l’album ${a.title}`}>Supprimer</button>
              <button class="v-btn sm ghost" onclick={() => void supprimer(a, true)} aria-label={`Supprimer l’artiste ${a.artist.name}`}>Supprimer l’artiste</button>
            </li>
          {/each}
        </ul>
      {/if}
    </section>
  {:else if onglet === 'synchro'}
    <section class="film-section">
      {#if statut}
        <div class="sillon-tuiles">
          {#each [['Artistes', statut.library.artists], ['Albums', statut.library.albums], ['Titres', statut.library.tracks], ['Titres absents', statut.library.absent_tracks], ['Fichiers', statut.library.files], ['Écartés', statut.library.rejected]] as [l, n] (l)}
            <div class="v-card sillon-tuile"><span class="v-eyebrow">{l}</span><strong class="sillon-chiffre">{n}</strong></div>
          {/each}
          <div class="v-card sillon-tuile"><span class="v-eyebrow">Taille</span><strong class="sillon-chiffre">{octets(statut.library.size)}</strong></div>
        </div>
        <div class="sillon-ligne-actions">
          <span class="v-tag" class:ok={statut.configured.lidarr} class:dgr={!statut.configured.lidarr}>Lidarr {statut.configured.lidarr ? 'configuré' : 'non configuré'}</span>
          <span class="v-tag" class:ok={statut.configured.webhook}>Webhook {statut.configured.webhook ? 'actif' : 'coupé'}</span>
          {#if statut.running}<span class="v-tag att">Synchronisation en cours</span>{/if}
          <button class="plbls-button plbls-button--compact" disabled={statut.running} onclick={() => void synchroniser()}><Icon name="actualise" size={14} /> Synchroniser</button>
        </div>
        <ul class="sillon-admin-liste">
          {#each statut.sync as s (s.source)}
            <li><span class="grow sillon-admin-t"><strong>{s.source}</strong><span class="v-note">Réussie : {date(s.succeeded_at)} · tentée : {date(s.attempted_at)}</span>{#if s.error}<span class="v-note sillon-erreur">{s.error}</span>{/if}</span><span class="v-tag" class:ok={!s.error} class:dgr={!!s.error}>{s.error ? 'Erreur' : 'OK'}</span></li>
          {/each}
        </ul>
      {:else}<Loader />{/if}
    </section>
  {:else if onglet === 'analyse'}
    <section class="film-section">
      {#if analyse}
        <div class="sillon-tuiles">
          <div class="v-card sillon-tuile"><span class="v-eyebrow">Analysés</span><strong class="sillon-chiffre">{analyse.analysed} / {analyse.tracks}</strong></div>
          <div class="v-card sillon-tuile"><span class="v-eyebrow">En attente</span><strong class="sillon-chiffre">{analyse.tasks.pending}</strong></div>
          <div class="v-card sillon-tuile"><span class="v-eyebrow">En cours</span><strong class="sillon-chiffre">{analyse.tasks.running}</strong></div>
          <div class="v-card sillon-tuile"><span class="v-eyebrow">En erreur</span><strong class="sillon-chiffre">{analyse.tasks.failed}</strong></div>
        </div>
        <div class="sillon-ligne-actions">
          {#if analyse.paused}<span class="v-tag att">En pause pendant un transcodage</span>{/if}
          <span class="v-note">Version {analyse.version} · {analyse.parallel} en parallèle · {analyse.loaded} en mémoire</span>
          <button class="plbls-button plbls-button--compact" disabled={!analyse.tasks.failed} onclick={() => void relancerAnalyse()}><Icon name="actualise" size={14} /> Relancer les erreurs</button>
        </div>
        {#if analyse.errors.length}
          <ul class="sillon-admin-liste">
            {#each analyse.errors as e (e.track)}
              <li><span class="grow sillon-admin-t"><strong class="truncate">{e.title}</strong><span class="v-note">{e.error ?? 'Erreur'} · {e.attempts} essai{e.attempts > 1 ? 's' : ''} · {date(e.at)}</span></span></li>
            {/each}
          </ul>
        {/if}
      {:else}<Loader />{/if}
    </section>
  {:else if onglet === 'ecartes'}
    <section class="film-section">
      {#if !ecartes}
        <Loader />
      {:else if !ecartes.length}
        <div class="v-empty"><Icon name="ok" size={28} /><p class="v-note">Aucun fichier écarté.</p></div>
      {:else}
        <ul class="sillon-admin-liste">
          {#each ecartes as f, i (i)}
            <li><span class="grow sillon-admin-t"><strong class="truncate">{f.name}</strong><span class="v-note">{RAISONS[f.reason] ?? f.reason} · {date(f.seen_at)}</span></span></li>
          {/each}
        </ul>
      {/if}
    </section>
  {:else}
    <section class="film-section">
      {#if decouverte}
        {@const d = decouverte}
        <div class="v-card sillon-disque-carte">
          <p class="v-eyebrow">Budget de la découverte</p>
          <p class="sillon-chiffre">{octets(d.budget.used)} sur {octets(d.budget.limit)}</p>
          {#if d.budget.limit}<div class="v-prog rose" role="progressbar" aria-label="Budget de la découverte" aria-valuenow={Math.round(Math.min(100, (d.budget.used / d.budget.limit) * 100))} aria-valuemin="0" aria-valuemax="100"><i style="width:{Math.min(100, (d.budget.used / d.budget.limit) * 100)}%"></i></div>{/if}
          <p class="v-note"><span class="v-tag" class:ok={d.enabled} class:att={!d.enabled}>{d.enabled ? 'Active' : 'Inactive'}</span> {d.budget.daily_limit} ajout{d.budget.daily_limit > 1 ? 's' : ''} par personne et par jour · essai de {d.budget.trial_days} jours avant de proposer la suppression</p>
        </div>
        <h2 class="v-titre sillon-sous-titre">À supprimer</h2>
        {#if d.removal_candidates.length}
          <p class="v-note sillon-aide-admin">Jamais écoutés en entier après l’essai, ou sautés trois fois.</p>
          <ul class="sillon-admin-liste">
            {#each d.removal_candidates as c (`${c.addition}-${c.album.id}`)}
              <li>
                <span class="sillon-admin-art"><SillonCover id={c.album.id} title={c.album.title} image={c.album.cover} size="small" /></span>
                <span class="grow sillon-admin-t"><strong class="truncate">{c.album.title}</strong><span class="v-note truncate">{[c.album.artist.name, octets(c.size), c.similar_to ? `proche de ${c.similar_to}` : '', `pour ${c.author.name}`].filter(Boolean).join(' · ')}</span><span class="v-note">{[c.skips ? `${c.skips} saut${c.skips > 1 ? 's' : ''}` : 'jamais écouté', c.available_since ? `présent depuis le ${date(c.available_since)}` : ''].filter(Boolean).join(' · ')}</span></span>
                <button class="v-btn sm sec" onclick={() => void supprimer(c.album, false, 'decouverte')} aria-label={`Supprimer l’album ${c.album.title}`}>Supprimer</button>
              </li>
            {/each}
          </ul>
        {:else}<p class="v-note">Aucune découverte à supprimer.</p>{/if}
        <h2 class="v-titre sillon-sous-titre">Ajouts de la découverte</h2>
        {#if d.items.length}
          <ul class="sillon-admin-liste">
            {#each d.items as a (a.id)}
              <li>
                <span class="grow sillon-admin-t"><strong class="truncate">{a.title}</strong><span class="v-note truncate">{[a.artist.name, octets(a.size || a.estimated_size), a.similar_to ? `proche de ${a.similar_to}` : '', a.track_title ? `pour « ${a.track_title} »` : '', a.author.name, date(a.created_at)].filter(Boolean).join(' · ')}</span>{#if a.reason}<span class="v-note sillon-erreur">{a.reason}</span>{/if}</span>
                <span class="v-tag" class:ok={a.state === 'available'} class:att={a.state === 'downloading' || a.state === 'sent' || a.state === 'partial'} class:dgr={a.state === 'failed'}>{ETATS_AJOUT[a.state]}</span>
              </li>
            {/each}
          </ul>
        {:else}<p class="v-note">Aucun ajout de la découverte.</p>{/if}
      {:else}<Loader />{/if}
    </section>
  {/if}
</div>

<style>
  .sillon-onglets { margin: 0 0 8px; overflow-x: auto; scrollbar-width: none; }
  .sillon-onglets > button { flex: none; }
  .sillon-disque-carte { display: grid; gap: 10px; max-width: 640px; }
  .sillon-disque-carte p { margin: 0; }
  .sillon-chiffre { font-family: var(--f-titre); font-size: 26px; font-weight: 700; letter-spacing: -0.02em; }
  .sillon-tuiles { display: grid; grid-template-columns: repeat(auto-fill, minmax(150px, 1fr)); gap: 12px; margin-bottom: 16px; }
  .sillon-tuile { display: grid; gap: 4px; padding: 14px 16px; }
  .sillon-ligne-actions { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; margin-bottom: 12px; }
  .sillon-filtre { display: inline-flex; align-items: center; gap: 8px; margin-bottom: 12px; font-size: 14px; cursor: pointer; }
  .sillon-filtre input { width: 18px; height: 18px; accent-color: var(--rose-fonce); }
  .sillon-admin-liste { display: flex; flex-direction: column; gap: 2px; margin: 0; padding: 0; list-style: none; }
  .sillon-admin-liste > li { display: flex; flex-wrap: wrap; align-items: center; gap: 6px 12px; min-height: 52px; padding: 6px 10px; border-radius: var(--r); }
  .sillon-admin-liste > li:hover { background: var(--papier-2); }
  .sillon-admin-art { position: relative; flex: none; width: 40px; height: 40px; overflow: hidden; border-radius: var(--r-s); }
  .sillon-admin-t { display: grid; flex: 1 1 220px; min-width: 0; line-height: 1.35; }
  .sillon-erreur { color: var(--danger); }
  .sillon-sous-titre { margin: 20px 0 8px; font-size: 20px; }
  .sillon-aide-admin { margin: 0 0 8px 10px; }
</style>
