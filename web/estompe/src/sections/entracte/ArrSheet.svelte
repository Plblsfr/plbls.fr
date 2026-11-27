<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import Modal from '../../components/Modal.svelte'
  import { entracteApi } from '../../lib/api'
  import { messageOf, notify } from '../../lib/ui'
  import { bytes, episodeCode, eventLabel, eventTone, serviceName, since, titleLabel, titleState, type ArrEpisode, type ArrHistory, type ArrQueueItem, type ArrSeason, type ArrService, type ArrSheet, type ReleaseTarget } from './arr'

  let { service, id, queue, onclose, onreleases, onchanged }: { service: ArrService; id: number; queue: ArrQueueItem[]; onclose: () => void; onreleases: (t: ReleaseTarget) => void; onchanged: () => void } = $props()

  let sheet = $state<ArrSheet | null>(null)
  let history = $state<ArrHistory[]>([])
  let error = $state('')
  let busy = $state('')
  let open = $state<number | null>(null)
  let deleting = $state(false)
  let del = $state({ files: false, exclude: false, reason: '' })
  let delError = $state('')

  const nom = $derived(serviceName(service))
  const label = $derived(sheet ? titleLabel(sheet) : '')
  const etat = $derived(sheet ? titleState(sheet, queue) : null)
  const episodes = (n: number) => (sheet?.episode_list ?? []).filter((e) => e.season === n).sort((a, b) => a.number - b.number)
  const saisons = $derived((sheet?.seasons ?? []).toSorted((a, b) => (a.number === 0 ? 1 : b.number === 0 ? -1 : a.number - b.number)))
  const enCours = (e: ArrEpisode) => queue.find((q) => q.service === 'sonarr' && q.episode?.id === e.id)

  async function load() {
    try {
      sheet = await entracteApi.arrSheet(service, id)
      error = ''
    } catch (e) { error = messageOf(e, `${nom} ne donne pas cette fiche.`) }
    try { history = (await entracteApi.arrHistory({ service, title: id })).items.slice(0, 30) } catch { history = [] }
  }
  $effect(() => { void service; void id; void load() })

  async function update(cle: string, corps: Parameters<typeof entracteApi.arrUpdate>[2], dit: string) {
    busy = cle
    try { sheet = await entracteApi.arrUpdate(service, id, corps); notify.ok(dit); onchanged() } catch (e) { notify.error(e, 'La modification n’a pas été enregistrée'); await load() } finally { busy = '' }
  }

  async function monitorEpisode(e: ArrEpisode, monitored: boolean) {
    busy = `e${e.id}`
    try { await entracteApi.arrMonitorEpisodes([e.id], monitored); e.monitored = monitored } catch (err) { notify.error(err, 'La surveillance n’a pas été enregistrée'); await load() } finally { busy = '' }
  }

  async function search(cle: string, corps: Parameters<typeof entracteApi.arrSearch>[1], quoi: string) {
    busy = cle
    try { await entracteApi.arrSearch(service, corps); notify.ok(`Recherche lancée : ${quoi}.`) } catch (e) { notify.error(e, 'La recherche n’a pas pu être lancée') } finally { busy = '' }
  }

  async function remove() {
    busy = 'suppression'
    delError = ''
    try {
      await entracteApi.arrDelete(service, id, del)
      notify.ok(`« ${label} » retiré de ${nom}.`)
      onchanged()
      onclose()
    } catch (e) { delError = messageOf(e, 'Le titre n’a pas pu être supprimé.') } finally { busy = '' }
  }

  const saisonNom = (s: ArrSeason) => (s.number === 0 ? 'Épisodes spéciaux' : `Saison ${s.number}`)
  const fichier = (e: ArrEpisode) => {
    const q = enCours(e)
    if (q) return `téléchargement à ${q.progress} %`
    if (e.has_file) return [e.quality, bytes(e.size)].filter(Boolean).join(' · ')
    return e.air_date && new Date(e.air_date).getTime() > Date.now() ? `diffusion ${since(e.air_date)}` : ''
  }
  const episodeEtat = (e: ArrEpisode): [string, string] => {
    const q = enCours(e)
    if (q) return q.state === 'blocked' ? ['Bloqué', 'dgr'] : ['En cours', 'info']
    if (e.has_file) return ['Présent', 'ok']
    if (e.air_date && new Date(e.air_date).getTime() > Date.now()) return ['À venir', '']
    return e.monitored ? ['Manquant', 'att'] : ['Non surveillé', '']
  }
</script>

<div class="film-page">
  {#if !sheet}
    <div class="film-filters"><button class="plbls-button plbls-button--compact plbls-button--contour" onclick={onclose}><Icon name="gauche" size={14} /> Retour</button></div>
    {#if error}<div class="plbls-alert plbls-alert--erreur" role="alert"><p class="plbls-alert__corps">{error}</p></div>{:else}<Loader />{/if}
  {:else}
    <div class="adm-tete">
      <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={onclose}><Icon name="gauche" size={14} /> Retour</button>
      <div class="adm-tete-corps">
        <span class="v-eyebrow">{service === 'radarr' ? 'Film' : 'Série'} · {nom}</span>
        <h2 class="v-titre">{label}</h2>
        <div class="adm-tags">
          <span class="v-tag {sheet.monitored ? 'info' : ''}">{sheet.monitored ? 'Surveillé' : 'Non surveillé'}</span>
          {#if etat}<span class="v-tag {etat.tone}">{etat.label}</span>{/if}
          {#if service === 'sonarr'}<span class="v-tag">{sheet.episode_files ?? 0} épisode{(sheet.episode_files ?? 0) > 1 ? 's' : ''} sur {sheet.episodes ?? 0}</span>{/if}
          {#if sheet.file?.quality}<span class="v-tag encre">{sheet.file.quality}</span>{/if}
          <span class="v-note">{[bytes(sheet.size), sheet.folder].filter(Boolean).join(' · ')}</span>
        </div>
        <div class="adm-actions">
          <button class="plbls-button plbls-button--compact" disabled={busy === 'titre'} onclick={() => void search('titre', { title: id }, label)}><Icon name="cherche" size={14} /> {service === 'radarr' ? 'Recherche automatique' : 'Rechercher toute la série'}</button>
          {#if service === 'radarr'}<button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => onreleases({ service: 'radarr', title: id, label })}>Choisir une release…</button>{/if}
          <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => void load()}><Icon name="actualise" size={14} /> Actualiser</button>
          <button class="plbls-button plbls-button--compact plbls-button--contour plbls-button--danger" onclick={() => { delError = ''; deleting = true }}>Supprimer…</button>
        </div>
      </div>
    </div>

    <section class="adm-champs" aria-label="Réglages">
      <label class="adm-champ"><span>Profil de qualité</span>
        <select class="plbls-select" value={sheet.profile} disabled={busy === 'profil'} onchange={(e) => void update('profil', { profile: Number(e.currentTarget.value) }, 'Profil de qualité changé.')}>
          {#each sheet.profiles as p (p.id)}<option value={p.id}>{p.name || `Profil ${p.id}`}</option>{/each}
        </select>
      </label>
      <label class="adm-coche"><input class="adm-case" type="checkbox" role="switch" checked={sheet.monitored} disabled={busy === 'surveille'} onchange={(e) => void update('surveille', { monitored: e.currentTarget.checked }, e.currentTarget.checked ? 'Surveillance activée.' : 'Surveillance coupée.')} />
        <span class="adm-body"><span class="v-lbl">Surveiller {service === 'radarr' ? 'le film' : 'la série'}</span><span class="v-note">{nom} récupère seul ce qui sort ou s’améliore.</span></span>
      </label>
      {#if service === 'sonarr'}
        <label class="adm-coche"><input class="adm-case" type="checkbox" role="switch" checked={sheet.monitor_new} disabled={busy === 'nouvelles'} onchange={(e) => void update('nouvelles', { monitor_new: e.currentTarget.checked }, 'Réglage des nouvelles saisons enregistré.')} />
          <span class="adm-body"><span class="v-lbl">Surveiller les nouvelles saisons</span><span class="v-note">Une saison annoncée plus tard est récupérée sans nouvelle demande.</span></span>
        </label>
      {/if}
    </section>

    {#if service === 'sonarr'}
      <section class="film-section">
        <h2 class="v-titre">Saisons</h2>
        <ul class="adm-list">
          {#each saisons as s (s.number)}
            <li class="adm-item adm-pile">
              <div class="adm-ligne">
                <input class="adm-case" type="checkbox" checked={s.monitored} disabled={busy === `s${s.number}`} aria-label={`Surveiller : ${saisonNom(s)}`} onchange={(e) => void update(`s${s.number}`, { seasons: [{ number: s.number, monitored: e.currentTarget.checked }] }, `${saisonNom(s)} ${e.currentTarget.checked ? 'surveillée' : 'non surveillée'}.`)} />
                <strong class="grow">{saisonNom(s)}</strong>
                <span class="v-tag {s.episode_files >= s.episodes && s.episodes > 0 ? 'ok' : s.monitored && s.episodes > 0 ? 'att' : ''}">{s.episode_files} sur {s.episodes}</span>
                {#if s.number > 0}
                  <button class="plbls-button plbls-button--compact plbls-button--contour" disabled={busy === `rs${s.number}`} onclick={() => void search(`rs${s.number}`, { title: id, season: s.number }, `${label}, saison ${s.number}`)}>Rechercher la saison</button>
                  <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => onreleases({ service: 'sonarr', title: id, season: s.number, label: `${label} · Saison ${s.number}` })}>Choisir un pack…</button>
                {/if}
                <button class="plbls-button plbls-button--compact plbls-button--contour" aria-expanded={open === s.number} onclick={() => (open = open === s.number ? null : s.number)}>Épisodes <Icon name={open === s.number ? 'bas' : 'droite'} size={14} /></button>
              </div>
              {#if open === s.number}
                <ul class="adm-sous">
                  {#each episodes(s.number) as e (e.id)}
                    {@const code = episodeCode(e)}
                    {@const [mot, ton] = episodeEtat(e)}
                    <li>
                      <input class="adm-case" type="checkbox" checked={e.monitored} disabled={busy === `e${e.id}`} aria-label={`Surveiller ${code}`} onchange={(ev) => void monitorEpisode(e, ev.currentTarget.checked)} />
                      <span class="adm-code">{code}</span>
                      <span class="grow">{e.title ?? ''}</span>
                      <span class="v-note">{fichier(e)}</span>
                      <span class="v-tag {ton}">{mot}</span>
                      <button class="plbls-button plbls-button--compact plbls-button--contour" disabled={busy === `re${e.id}`} aria-label={`Rechercher ${code}`} onclick={() => void search(`re${e.id}`, { episodes: [e.id] }, `${label} ${code}`)}>Rechercher</button>
                      <button class="plbls-button plbls-button--compact plbls-button--contour" aria-label={`Choisir une release pour ${code}`} onclick={() => onreleases({ service: 'sonarr', episode: e.id, label: `${label} · ${code}` })}>Choisir…</button>
                    </li>
                  {:else}
                    <li><span class="v-note">Aucun épisode connu.</span></li>
                  {/each}
                </ul>
              {/if}
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    <section class="film-section">
      <h2 class="v-titre">Historique</h2>
      {#if !history.length}
        <p class="v-note">Aucune récupération enregistrée pour ce titre.</p>
      {:else}
        <ul class="adm-list">
          {#each history as h, i (i)}
            <li class="adm-item">
              <span class="v-tag {eventTone(h.event)}">{eventLabel(h.event)}</span>
              {#if episodeCode(h.episode)}<span class="adm-code">{episodeCode(h.episode)}</span>{/if}
              <span class="grow adm-body"><span class="adm-mono v-mute">{h.release ?? ''}</span>{#if h.reason}<span class="v-note">{h.reason}</span>{/if}</span>
              <span class="v-note">{[h.quality, h.indexer, since(h.date)].filter(Boolean).join(' · ')}</span>
            </li>
          {/each}
        </ul>
      {/if}
    </section>
  {/if}
</div>

{#if deleting && sheet}
  <Modal title="Supprimer « {label} »" onclose={() => (deleting = false)}>
    <form class="sup" onsubmit={(e) => { e.preventDefault(); void remove() }}>
      <p class="v-note">Le titre est retiré de {nom}. Les demandes en cours qui le concernent passent à « refusée » et leurs demandeurs sont prévenus.</p>
      <label class="adm-coche"><input class="adm-case" type="checkbox" bind:checked={del.files} /> Supprimer aussi les fichiers{bytes(sheet.size) ? ` (${bytes(sheet.size)})` : ''}</label>
      <label class="adm-coche"><input class="adm-case" type="checkbox" bind:checked={del.exclude} /> Empêcher une liste d’import de le rajouter</label>
      <label class="adm-champ"><span>Motif envoyé aux demandeurs</span><input class="plbls-input" maxlength="500" placeholder="Titre retiré du catalogue" bind:value={del.reason} /></label>
      {#if delError}<div class="plbls-alert plbls-alert--erreur" role="alert"><p class="plbls-alert__corps">{delError}</p></div>{/if}
      <div class="sup-actions">
        <button type="button" class="plbls-button plbls-button--contour" onclick={() => (deleting = false)}>Annuler</button>
        <button type="submit" class="plbls-button plbls-button--danger" disabled={busy === 'suppression'}><Icon name="corbeille" size={16} /> Supprimer de {nom}</button>
      </div>
    </form>
  </Modal>
{/if}

<style>
  .sup { display: flex; flex-direction: column; gap: var(--espace-3); }
  .sup-actions { display: flex; justify-content: flex-end; flex-wrap: wrap; gap: var(--espace-2); }
</style>
