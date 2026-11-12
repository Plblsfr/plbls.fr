<script lang="ts">
  import Icon from '../../../../web/estompe/src/components/Icon.svelte'
  import { notify } from '../../../../web/estompe/src/lib/ui'
  import { invoke } from '../../crates/core/ts/tauri'
  import { overlayAccelerator } from './overlay'
  import { checkGlobalPtt } from './ptt-global'
  import { ajouterJeu, CADENCES, CODECS, DUREE_MAX_S, DUREE_MIN_S, dureeTampon, nomExecutable, partUtilisee, QUALITES, QUOTA_MIN_GO, raccourciLisible, RESOLUTIONS, taille, type Qualite } from './replay'
  import { chargerClips, chargerMateriel, ouvrirDossier, regler, replay, texteErreur } from './replay.svelte'

  void chargerMateriel()
  void chargerClips()

  const r = $derived(replay.reglages)
  const o = $derived(replay.options)
  const pret = $derived(!!replay.dispo?.disponible)
  const resolution = (id: string) => o?.resolutions.find((x) => x.id === id)
  const cadence = (ips: number) => o?.cadences.find((x) => x.ips === ips)
  const dureeMax = $derived(Math.max(DUREE_MIN_S, Math.min(DUREE_MAX_S, o?.dureeMaxS ?? DUREE_MAX_S)))
  const tourne = $derived(replay.etat.etat === 'arme' || replay.etat.etat === 'pause')
  const horsPortee = $derived([resolution(r.resolution), cadence(r.ips)].filter((x) => x && !x.disponible).map((x) => x!.raison).filter(Boolean))
  const etat = $derived.by(() => {
    const e = replay.etat
    if (!r.actif) return 'Désactivé.'
    if (e.etat === 'arme') return `Le tampon tourne${e.jeu ? ` sur ${e.jeu}` : ''}${e.codec ? ` : ${CODECS[e.codec]}, ${e.largeur} × ${e.hauteur}, ${e.ips} i/s` : ''}.`
    if (e.etat === 'pause') return `En pause${e.raison ? ` : ${e.raison}` : ''}. Le tampon est gardé.`
    if (e.etat === 'indisponible') return `Indisponible${e.raison ? ` : ${e.raison}` : ''}.`
    return 'En attente : le tampon s’arme dès qu’un jeu détecté passe au premier plan.'
  })

  let capture = $state(false)
  async function capter(e: KeyboardEvent) {
    e.preventDefault()
    const touche = overlayAccelerator(e)
    if (!touche) return
    capture = false
    if (touche === r.raccourci) return
    const refus = await checkGlobalPtt(touche)
    if (refus) return notify.info(refus)
    regler({ raccourci: touche })
  }

  const choisir = (commande: string) => invoke<string | null>(commande).catch((e) => { notify.fail(texteErreur(e)); return null })
  async function ajouter() {
    const chemin = await choisir('replay_choisir_executable')
    if (chemin) regler({ jeuxAjoutes: ajouterJeu(r.jeuxAjoutes, chemin) })
  }
  let exclu = $state('')
  function exclure(nom: string) {
    regler({ jeuxExclus: ajouterJeu(r.jeuxExclus, nomExecutable(nom)) })
    exclu = ''
  }
  async function exclureFichier() {
    const chemin = await choisir('replay_choisir_executable')
    if (chemin) exclure(chemin)
  }
  async function changerDossier() {
    const dossier = await choisir('replay_choisir_dossier')
    if (dossier) regler({ dossier })
  }
  const nombre = (e: Event, min: number, defaut: number) => Math.max(min, Math.round(Number((e.currentTarget as HTMLInputElement).value)) || defaut)
</script>

<section class="p-section">
  <h3 class="p-sec">Activation <span class="plbls-badge">bêta</span></h3>
  <p class="v-note">Garde en mémoire les dernières secondes de ton jeu et les enregistre en clip sur un raccourci. Rien n’est écrit sur le disque tant que tu ne sauvegardes pas, et aucun clip ne quitte ton PC sans action de ta part. Fonction en bêta : vérifiée sur carte graphique AMD seulement.</p>
  {#if replay.dispo && !replay.dispo.disponible}
    <div class="plbls-alert plbls-alert--erreur" role="alert">
      <p class="plbls-alert__titre">Instant replay indisponible sur cet appareil</p>
      <p class="plbls-alert__corps">{replay.dispo.raison ?? 'Aucun encodeur vidéo matériel n’a été trouvé.'} L’encodage par le processeur n’est pas proposé : il ferait chuter les images par seconde du jeu.</p>
    </div>
  {/if}
  <div class="p-carte">
    <label class="plbls-toggle">
      <input class="plbls-toggle__input" type="checkbox" checked={r.actif} disabled={!pret && !r.actif} onchange={(e) => regler({ actif: e.currentTarget.checked })} aria-describedby="rp-etat" />
      <span class="plbls-toggle__piste"></span>
      <span class="plbls-toggle__texte">Activer l’instant replay</span>
    </label>
    <p class="v-note rp-note" id="rp-etat" role="status">{#if !replay.dispo}Lecture du matériel…{:else}{etat}{replay.dispo.encodeur ? ` Encodeur : ${replay.dispo.encodeur}.` : ''}{/if}</p>
    <div class="p-ligne">
      <span class="p-grow">Raccourci de sauvegarde : <kbd class="v-kbd">{raccourciLisible(r.raccourci)}</kbd></span>
      {#if capture}
        <input class="plbls-input" readonly placeholder="Appuie sur le raccourci…" onkeydown={capter} onblur={() => (capture = false)} {@attach (n) => n.focus()} />
      {:else}
        <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => (capture = true)}>Changer</button>
      {/if}
    </div>
    <p class="v-note rp-note">Le raccourci fonctionne en jeu, seulement quand l’instant replay est activé. Deux appuis rapprochés donnent deux clips.</p>
  </div>
</section>

{#if pret}
  <section class="p-section">
    <h3 class="p-sec">Vidéo</h3>
    <div class="v-alert" class:att={tourne} role="note"><Icon name={tourne ? 'alerte' : 'info'} size={18} /><span>Changer la résolution, les images par seconde, la durée, la qualité, le micro ou le curseur vide le tampon{tourne ? ' en cours' : ''} : ce qui précède le changement est perdu.</span></div>
    <div class="p-carte">
      <div class="p-ligne">
        <span class="p-grow" id="rp-res">Résolution</span>
        <div class="v-seg" role="radiogroup" aria-labelledby="rp-res">
          {#each RESOLUTIONS as [id, nom] (id)}
            {@const c = resolution(id)}
            <button role="radio" aria-checked={r.resolution === id} aria-disabled={c ? !c.disponible : undefined} class:gris={c && !c.disponible} title={c?.disponible === false ? (c.raison ?? 'Hors de portée') : undefined} onclick={() => { if (c?.disponible !== false) regler({ resolution: id }) }}>{nom}</button>
          {/each}
        </div>
      </div>
      <div class="p-ligne">
        <span class="p-grow" id="rp-ips">Images par seconde</span>
        <div class="v-seg" role="radiogroup" aria-labelledby="rp-ips">
          {#each CADENCES as ips (ips)}
            {@const c = cadence(ips)}
            <button role="radio" aria-checked={r.ips === ips} aria-disabled={c ? !c.disponible : undefined} class:gris={c && !c.disponible} title={c?.disponible === false ? (c.raison ?? 'Hors de portée') : undefined} onclick={() => { if (c?.disponible !== false) regler({ ips }) }}>{ips}</button>
          {/each}
        </div>
      </div>
      {#if horsPortee.length}<p class="v-note rp-note" role="status">Réglage hors de portée ({horsPortee.join(' ; ')}) : le moteur applique le plus proche.</p>{/if}
      <div class="p-ligne">
        <label class="p-grow" for="rp-qualite">Qualité</label>
        <select id="rp-qualite" class="plbls-select" value={r.qualite} onchange={(e) => regler({ qualite: e.currentTarget.value as Qualite })}>
          {#each QUALITES as [id, nom] (id)}<option value={id}>{nom}</option>{/each}
        </select>
      </div>
      <div class="p-ligne">
        <label class="p-grow" for="rp-duree">Durée gardée : <span class="v-mono">{dureeTampon(Math.min(r.dureeS, dureeMax))}</span></label>
        <input id="rp-duree" type="range" min={DUREE_MIN_S} max={dureeMax} step="15" value={Math.min(r.dureeS, dureeMax)} onchange={(e) => regler({ dureeS: Number(e.currentTarget.value) })} />
      </div>
      {#if dureeMax < DUREE_MAX_S}<p class="v-note rp-note" role="status" title={o?.raisonDuree ?? undefined}>Durée plafonnée à {dureeTampon(dureeMax)} au lieu de {dureeTampon(DUREE_MAX_S)}{o?.raisonDuree ? ` : ${o.raisonDuree}` : ''}.</p>{/if}
      <label class="plbls-toggle">
        <input class="plbls-toggle__input" type="checkbox" checked={r.curseur} onchange={(e) => regler({ curseur: e.currentTarget.checked })} />
        <span class="plbls-toggle__piste"></span>
        <span class="plbls-toggle__texte">Afficher le curseur dans le clip</span>
      </label>
      {#if o}
        <p class="v-note rp-note" role="status">Codec utilisé : <strong>{CODECS[o.codec]}</strong>, environ {Math.round(o.debitKbps / 100) / 10} Mbit/s ; mémoire du tampon : environ {taille(o.memoireOctets)}.{#if o.codec === 'hevc'} Ce réglage dépasse ce que l’encodeur H.264 sait faire : un clip HEVC est converti en H.264 à l’envoi dans un salon.{/if}</p>
      {:else if replay.erreurOptions}
        <p class="v-note rp-note" role="alert">Plafonds du matériel illisibles : {replay.erreurOptions}</p>
      {/if}
    </div>
  </section>

  <section class="p-section">
    <h3 class="p-sec">Son</h3>
    <div class="p-carte">
      <label class="plbls-toggle">
        <input class="plbls-toggle__input" type="checkbox" checked={r.micro} onchange={(e) => regler({ micro: e.currentTarget.checked })} aria-describedby="rp-micro" />
        <span class="plbls-toggle__piste"></span>
        <span class="plbls-toggle__texte">Enregistrer mon micro</span>
      </label>
      <p class="v-note rp-note" id="rp-micro">Une fois allumé, ton micro est capté en continu dès que le tampon tourne, <strong>même quand tu es muet dans Estompe</strong> ou hors d’un salon vocal. Il part sur une piste séparée du son du jeu, avec le micro, la réduction de bruit et le volume choisis dans « Voix et vidéo ». Éteint, le micro n’est jamais ouvert. La voix des autres participants n’est jamais enregistrée.</p>
      {#if r.micro && replay.etat.microParDefaut}
        <div class="v-alert att" role="status"><Icon name="alerte" size={18} /><span>Le micro choisi dans « Voix et vidéo » n’a pas été retrouvé par Windows : c’est le micro par défaut du système qui est enregistré.</span></div>
      {/if}
    </div>
  </section>

  <section class="p-section">
    <h3 class="p-sec">Jeux</h3>
    <div class="p-carte">
      <p class="v-note rp-note">Le tampon ne tourne que sur un jeu détecté au premier plan. Ajoute un jeu qui n’est pas reconnu, ou exclus-en un pour qu’il n’arme jamais le tampon.</p>
      <div class="p-ligne">
        <strong class="p-grow">Jeux ajoutés à la main</strong>
        <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={ajouter}><Icon name="plus" size={16} />Ajouter un jeu…</button>
      </div>
      {#each r.jeuxAjoutes as chemin (chemin)}
        <div class="rp-jeu">
          <span class="p-grow truncate" title={chemin}>{nomExecutable(chemin)}{' '}<span class="v-note">{chemin}</span></span>
          <button class="v-ib" aria-label="Retirer {nomExecutable(chemin)}" title="Retirer" onclick={() => regler({ jeuxAjoutes: r.jeuxAjoutes.filter((x) => x !== chemin) })}><Icon name="x" size={16} /></button>
        </div>
      {:else}
        <p class="v-note rp-note">Aucun jeu ajouté.</p>
      {/each}
      <div class="p-ligne">
        <strong class="p-grow">Jeux exclus</strong>
        <form class="row" onsubmit={(e) => { e.preventDefault(); if (exclu.trim()) exclure(exclu) }}>
          <input class="plbls-input rp-champ" placeholder="jeu.exe" aria-label="Nom de l’exécutable à exclure" bind:value={exclu} maxlength="260" />
          <button class="plbls-button plbls-button--compact plbls-button--contour" disabled={!exclu.trim()}>Exclure</button>
          <button type="button" class="plbls-button plbls-button--compact plbls-button--discret" onclick={exclureFichier}>Parcourir…</button>
        </form>
      </div>
      {#each r.jeuxExclus as nom (nom)}
        <div class="rp-jeu">
          <span class="p-grow truncate">{nom}</span>
          <button class="v-ib" aria-label="Ne plus exclure {nom}" title="Ne plus exclure" onclick={() => regler({ jeuxExclus: r.jeuxExclus.filter((x) => x !== nom) })}><Icon name="x" size={16} /></button>
        </div>
      {:else}
        <p class="v-note rp-note">Aucun jeu exclu.</p>
      {/each}
    </div>
  </section>
{/if}

<section class="p-section">
  <h3 class="p-sec">Clips et stockage</h3>
  <div class="p-carte">
    <div class="p-ligne">
      <span class="p-grow truncate" title={r.dossier ?? undefined}>Dossier des clips : <span class="v-mono">{r.dossier ?? 'Vidéos/Estompe'}</span>{r.dossier ? '' : ' (par défaut)'}</span>
      <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={changerDossier}>Changer…</button>
      {#if r.dossier}<button class="plbls-button plbls-button--compact plbls-button--discret" onclick={() => regler({ dossier: null })}>Par défaut</button>{/if}
      <button class="plbls-button plbls-button--compact plbls-button--discret" onclick={ouvrirDossier}><Icon name="external" size={16} />Ouvrir</button>
    </div>
    <div class="p-ligne">
      <label class="p-grow" for="rp-quota">Quota du dossier (Go)</label>
      <input id="rp-quota" class="plbls-input rp-champ" type="number" min={QUOTA_MIN_GO} step="1" value={r.quotaGo} onchange={(e) => regler({ quotaGo: nombre(e, QUOTA_MIN_GO, r.quotaGo) })} />
    </div>
    {#if replay.stockage}
      {@const s = replay.stockage}
      <div class="plbls-progress rp-usage">
        <div class="plbls-progress__entete"><span>Espace utilisé : {replay.clips.length} clip{replay.clips.length > 1 ? 's' : ''}</span><span>{taille(s.utiliseOctets)} sur {taille(s.quotaOctets)}</span></div>
        <div class="plbls-progress__piste" role="progressbar" aria-label="Espace utilisé" aria-valuemin="0" aria-valuemax="100" aria-valuenow={partUtilisee(s)}><div class="plbls-progress__remplissage" style:width="{partUtilisee(s)}%"></div></div>
      </div>
      {#if s.depasse}<div class="v-alert att" role="status"><Icon name="alerte" size={18} /><span>Quota atteint : plus aucun clip n’est enregistré. Supprime d’anciens clips depuis la galerie (rien n’est supprimé sans ta confirmation) ou augmente le quota.</span></div>{/if}
    {/if}
    <p class="v-note rp-note">La galerie des clips est dans la barre des serveurs, dès que l’instant replay est activé ou qu’un clip existe.</p>
  </div>
</section>

<style>
  .rp-note { margin: 0; padding: 8px 0; }
  .rp-champ { width: 140px; }
  .rp-jeu { display: flex; align-items: center; gap: 10px; min-width: 0; padding: 2px 0; }
  .rp-usage { padding: 12px 0; }
  .v-seg > .gris { opacity: 0.4; cursor: not-allowed; }
  .v-alert { margin: 8px 0; }
</style>
