<script lang="ts">
  import Icon, { type IconName } from '../../../../web/estompe/src/components/Icon.svelte'
  import Modal from '../../../../web/estompe/src/components/Modal.svelte'
  import { confirmer, saisir } from '../../../../web/estompe/src/lib/confirm.svelte'
  import { openSettings } from '../../../../web/estompe/src/lib/settings.svelte'
  import { notify } from '../../../../web/estompe/src/lib/ui'
  import { invoke } from '../../crates/core/ts/tauri'
  import type { Projet } from './montage'
  import { chargerProjets, editeur, ouvrirEditeur } from './montage.svelte'
  import ReplayEditeur from './ReplayEditeur.svelte'
  import ReplayEnvoi from './ReplayEnvoi.svelte'
  import { dateClip, detailsClip, dureeClip, jeuDe, PAGE_REGLAGES, partUtilisee, raccourciLisible, taille, trierClips, type Clip } from './replay'
  import { chargerClips, ouvrirDossier, renommerClip, replay, supprimerClips, texteErreur, urlFichier } from './replay.svelte'

  let { section, onmenu }: { section?: string; onmenu: () => void } = $props()

  void chargerClips()
  void chargerProjets()

  const clips = $derived(trierClips(replay.clips, section))
  let lu = $state<Clip | null>(null)
  let illisible = $state(false)
  let envoye = $state<Clip | null>(null)
  let sansVignette = $state<Record<string, boolean>>({})

  const tenter = (f: () => Promise<unknown>) => f().catch((e) => notify.fail(texteErreur(e)))
  async function renommer(c: Clip) {
    const nom = await saisir({ titre: 'Renommer le clip', action: 'Renommer', saisie: { label: 'Nom du clip', valeur: c.nom } })
    if (nom?.trim() && nom.trim() !== c.nom) await tenter(() => renommerClip(c, nom))
  }
  async function supprimer(c: Clip) {
    if (await confirmer({ titre: 'Supprimer le clip', message: `Supprimer définitivement « ${c.nom} » (${taille(c.tailleOctets)}) ? Le fichier est effacé du disque.`, action: 'Supprimer', destructif: true })) await tenter(() => supprimerClips([c.chemin]))
  }
  async function liberer() {
    const anciens = replay.stockage?.plusAnciens ?? []
    const n = anciens.length
    if (!n) return
    const octets = replay.clips.filter((c) => anciens.includes(c.chemin)).reduce((t, c) => t + c.tailleOctets, 0)
    if (await confirmer({ titre: 'Supprimer les clips les plus anciens', message: `Supprimer définitivement ${n > 1 ? `les ${n} clips les plus anciens` : 'le clip le plus ancien'} (${taille(octets)}) pour repasser sous le quota ?`, action: n > 1 ? `Supprimer ${n} clips` : 'Supprimer le clip', destructif: true })) await tenter(() => supprimerClips(anciens))
  }

  const ACTIONS: { id: string; label: string; icon: IconName; texte?: boolean; run: (c: Clip) => void }[] = [
    { id: 'envoyer', label: 'Envoyer dans un salon', icon: 'envoi', texte: true, run: (c) => (envoye = c) },
    { id: 'modifier', label: 'Modifier', icon: 'ciseaux', run: (c) => void ouvrirEditeur(c) },
    { id: 'renommer', label: 'Renommer', icon: 'crayon', run: (c) => void renommer(c) },
    { id: 'supprimer', label: 'Supprimer', icon: 'corbeille', run: (c) => void supprimer(c) },
  ]
  const lire = (c: Clip) => { illisible = false; lu = c }
  async function renommerProjet(p: Projet) {
    const nom = await saisir({ titre: 'Renommer le montage', action: 'Renommer', saisie: { label: 'Nom du montage', valeur: p.nom } })
    if (nom?.trim() && nom.trim() !== p.nom) await tenter(() => invoke('montage_renommer', { projet: p.id, nom }).then(chargerProjets))
  }
  async function supprimerProjet(p: Projet) {
    if (await confirmer({ titre: 'Supprimer le montage', message: `Supprimer le montage « ${p.nom} » ? Les clips qu’il utilise et les fichiers déjà exportés restent dans la galerie.`, action: 'Supprimer', destructif: true })) await tenter(() => invoke('montage_supprimer', { projet: p.id }).then(chargerProjets))
  }
  const sansExtension = (nom: string) => nom.replace(/\.mp4$/i, '')
</script>

{#if editeur.id}
  {#key editeur.id}<ReplayEditeur {onmenu} />{/key}
{:else}
<header class="main-head">
  <button class="ib burger" onclick={onmenu} aria-label="Menu"><Icon name="menu" /></button>
  <div class="main-head-t"><h1 class="truncate">{section ?? 'Clips'}</h1><span class="plbls-badge">bêta</span></div>
  <div class="main-head-actions">
    <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={ouvrirDossier}><Icon name="external" size={16} />Ouvrir le dossier</button>
    <button class="v-ib" aria-label="Réglages de l’instant replay" title="Réglages de l’instant replay" onclick={() => openSettings(PAGE_REGLAGES)}><Icon name="reglages" /></button>
  </div>
</header>
<div class="rg-page">
  {#if replay.stockage}
    {@const s = replay.stockage}
    <div class="plbls-progress rg-usage">
      <div class="plbls-progress__entete"><span>Espace utilisé</span><span>{taille(s.utiliseOctets)} sur {taille(s.quotaOctets)}</span></div>
      <div class="plbls-progress__piste" role="progressbar" aria-label="Espace utilisé" aria-valuemin="0" aria-valuemax="100" aria-valuenow={partUtilisee(s)}><div class="plbls-progress__remplissage" style:width="{partUtilisee(s)}%"></div></div>
    </div>
    {#if s.depasse}
      <div class="v-alert att" role="alert">
        <Icon name="alerte" size={18} />
        <span class="grow">Quota du dossier atteint : plus aucun clip n’est enregistré. Rien n’est supprimé sans ta confirmation.</span>
        {#if s.plusAnciens.length}<button class="plbls-button plbls-button--compact" onclick={liberer}>Supprimer les plus anciens…</button>{/if}
      </div>
    {/if}
  {/if}
  {#if editeur.preparation}
    {@const p = editeur.preparation}
    <div class="v-alert" role="status">
      <Icon name="ciseaux" size={18} />
      {#if p.pct === null}<span>Ouverture de « {p.nom} » dans l’éditeur…</span>{:else}{@render avancement(`Préparation de l’aperçu de « ${p.nom} »`, p.pct)}{/if}
    </div>
  {/if}
  {#each replay.travaux as t (t.id)}
    <div class="v-alert" role="status">
      <Icon name="envoie" size={18} />
      {#if t.pct === undefined}<span>{t.libelle}…</span>{:else}{@render avancement(t.libelle, t.pct)}{/if}
      {#if t.annuler}<button class="plbls-button plbls-button--compact plbls-button--contour" onclick={t.annuler}>Annuler</button>{/if}
    </div>
  {/each}
  {#if editeur.projets.length && !section}
    <section class="rg-projets" aria-label="Montages">
      <h2 class="side-title">Montages</h2>
      <ul>
        {#each editeur.projets as p (p.id)}
          <li class="rg-projet">
            <Icon name="ciseaux" size={18} />
            <div class="rg-projet-infos">
              <strong class="truncate" title={p.nom}>{p.nom}</strong>
              <span class="v-note truncate">{[`${p.clips.length} clip${p.clips.length > 1 ? 's' : ''}`, dureeClip(p.dureeS), dateClip(p.date)].filter(Boolean).join(' · ')}</span>
              {#if p.manquants.length}<span class="v-tag att" title={p.manquants.map(sansExtension).join(', ')}><Icon name="alerte" size={12} />{p.manquants.length > 1 ? `${p.manquants.length} clips introuvables` : `Clip introuvable : ${sansExtension(p.manquants[0])}`}</span>{/if}
            </div>
            <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => void ouvrirEditeur({ projet: p.id, nom: p.nom })}>Rouvrir</button>
            <button class="v-ib" aria-label="Renommer le montage {p.nom}" title="Renommer" onclick={() => void renommerProjet(p)}><Icon name="crayon" size={18} /></button>
            <button class="v-ib" aria-label="Supprimer le montage {p.nom}" title="Supprimer" onclick={() => void supprimerProjet(p)}><Icon name="corbeille" size={18} /></button>
          </li>
        {/each}
      </ul>
    </section>
  {/if}
  {#if clips.length}
    <ul class="rg-grille">
      {#each clips as c (c.chemin)}
        <li class="rg-clip">
          <button class="rg-vignette" onclick={() => lire(c)} aria-label="Lire {c.nom}">
            {#if c.vignette && !sansVignette[c.chemin]}<img src={urlFichier(c.vignette)} alt="" loading="lazy" onerror={() => (sansVignette[c.chemin] = true)} />{:else}<Icon name="film" size={32} />{/if}
            <span class="rg-lecture" aria-hidden="true"><Icon name="lecture" size={22} /></span>
            {#if c.dureeS}<span class="rg-duree v-mono">{dureeClip(c.dureeS)}</span>{/if}
          </button>
          <div class="rg-infos">
            <strong class="truncate" title={c.nom}>{c.nom}</strong>
            {#if c.origine}<span class="v-note truncate rg-montage" title="Montage d’après {c.origine}"><span class="plbls-badge">Montage</span>d’après {c.origine}</span>{/if}
            <span class="v-note truncate">{[jeuDe(c), dateClip(c.date)].filter(Boolean).join(' · ')}</span>
            <span class="v-note truncate">{detailsClip(c)}</span>
          </div>
          <div class="rg-actions">
            {#each ACTIONS as a (a.id)}
              {#if a.texte}
                <button class="plbls-button plbls-button--compact plbls-button--contour rg-grand" onclick={() => a.run(c)}><Icon name={a.icon} size={16} />{a.label}</button>
              {:else}
                <button class="v-ib" aria-label="{a.label} {c.nom}" title={a.label} onclick={() => a.run(c)}><Icon name={a.icon} size={18} /></button>
              {/if}
            {/each}
          </div>
        </li>
      {/each}
    </ul>
  {:else}
    <div class="v-empty">
      <Icon name="clap" size={32} />
      <strong>Aucun clip{section ? ' pour ce jeu' : ''}</strong>
      {#if replay.reglages.actif}
        <span class="v-note">En jeu, appuie sur <kbd class="v-kbd">{raccourciLisible(replay.reglages.raccourci)}</kbd> pour enregistrer les dernières secondes.</span>
      {:else}
        <span class="v-note">L’instant replay est désactivé.</span>
        <button class="plbls-button plbls-button--compact" onclick={() => openSettings(PAGE_REGLAGES)}>Ouvrir les réglages</button>
      {/if}
    </div>
  {/if}
</div>
{/if}

{#snippet avancement(libelle: string, pct: number)}
  <div class="plbls-progress grow">
    <div class="plbls-progress__entete"><span>{libelle}</span><span>{pct} %</span></div>
    <div class="plbls-progress__piste" role="progressbar" aria-label={libelle} aria-valuemin="0" aria-valuemax="100" aria-valuenow={pct}><div class="plbls-progress__remplissage" style:width="{pct}%"></div></div>
  </div>
{/snippet}

{#if lu}
  <Modal title={lu.nom} wide onclose={() => (lu = null)}>
    {#if illisible}
      <div class="rg-illisible">
        <div class="plbls-alert plbls-alert--erreur" role="alert">
          <p class="plbls-alert__titre">Lecture impossible dans Estompe</p>
          <p class="plbls-alert__corps">{lu.codec === 'hevc' ? 'Ce clip est en HEVC (H.265), que le lecteur intégré ne sait pas lire sur cet appareil. Ouvre le dossier pour le lire avec un autre lecteur ; « Envoyer dans un salon » le convertit en H.264.' : 'Le lecteur intégré ne sait pas lire ce fichier. Ouvre le dossier pour le lire avec un autre lecteur.'}</p>
        </div>
        <button class="plbls-button plbls-button--contour" onclick={ouvrirDossier}><Icon name="external" size={16} />Ouvrir le dossier</button>
      </div>
    {:else}
      {#key lu.chemin}<!-- svelte-ignore a11y_media_has_caption --><video class="rg-video" controls preload="metadata" src={urlFichier(lu.chemin)} onerror={() => (illisible = true)}></video>{/key}
    {/if}
  </Modal>
{/if}
{#if envoye}<ReplayEnvoi clip={envoye} onclose={() => (envoye = null)} />{/if}

<style>
  .rg-page { flex: 1; min-height: 0; overflow: auto; display: flex; flex-direction: column; gap: 16px; padding: 24px; scrollbar-width: thin; }
  .rg-usage { max-width: 420px; }
  .rg-page .v-alert { align-items: center; }
  .rg-projets ul { display: grid; grid-template-columns: repeat(auto-fill, minmax(340px, 1fr)); gap: 8px 16px; margin: 6px 0 0; padding: 0; list-style: none; }
  .rg-projets .side-title { margin: 0; padding: 0; }
  .rg-projet { display: flex; align-items: center; gap: 10px; min-width: 0; padding: 8px 8px 8px 14px; border-radius: var(--r-l); background: var(--surface); box-shadow: var(--o1); }
  .rg-projet-infos { display: flex; flex: 1; flex-direction: column; align-items: flex-start; gap: 2px; min-width: 0; }
  .rg-projet-infos > * { max-width: 100%; }
  .rg-grille { display: grid; grid-template-columns: repeat(auto-fill, minmax(260px, 1fr)); gap: 16px; margin: 0; padding: 0; list-style: none; }
  .rg-clip { display: flex; flex-direction: column; min-width: 0; border-radius: var(--r-l); background: var(--surface); box-shadow: var(--o1); overflow: hidden; }
  .rg-vignette { position: relative; display: grid; place-items: center; aspect-ratio: 16 / 9; padding: 0; border: 0; background: var(--papier-2); color: var(--gris); cursor: pointer; }
  .rg-vignette img { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; }
  .rg-vignette:focus-visible { outline: none; box-shadow: var(--anneau-in); }
  .rg-lecture { position: absolute; display: grid; place-items: center; width: 44px; height: 44px; border-radius: 50%; background: rgb(0 0 0 / 55%); color: #fff; opacity: 0; transition: opacity var(--d) var(--sortie); }
  .rg-vignette:hover .rg-lecture, .rg-vignette:focus-visible .rg-lecture { opacity: 1; }
  .rg-duree { position: absolute; right: 8px; bottom: 8px; padding: 2px 6px; border-radius: var(--r-xs); background: rgb(0 0 0 / 70%); color: #fff; font-size: 12px; }
  .rg-montage .plbls-badge { margin-right: 6px; }
  .rg-infos { display: flex; flex-direction: column; gap: 2px; min-width: 0; padding: 12px 14px 4px; }
  .rg-actions { display: flex; align-items: center; gap: 4px; padding: 6px 8px 10px 14px; }
  .rg-grand { margin-right: auto; }
  .rg-illisible { display: flex; flex-direction: column; align-items: flex-start; gap: 12px; }
  .rg-illisible .plbls-alert { margin: 0; align-self: stretch; }
  .rg-video { display: block; width: 100%; max-height: 70vh; border-radius: var(--r); background: #000; }
</style>
