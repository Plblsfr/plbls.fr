<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import { confirmer } from '../../lib/confirm.svelte'
  import { hrefs } from '../../lib/route.svelte'
  import { instant, periode, pluriel } from './formats'
  import TirelireFichier from './TirelireFichier.svelte'
  import { bilan, nomReleve, parGravite, type Releve } from './tirelire'
  import { tirelire } from './tirelire.svelte'

  let champ = $state<HTMLInputElement>()
  let survol = $state(false)
  let suppression = $state<number | null>(null)
  const b = $derived(bilan(tirelire.issues))
  const issues = $derived(parGravite(tirelire.issues))

  function envoyer(liste: FileList | null | undefined) {
    const fichiers = [...(liste ?? [])]
    if (fichiers.length) void tirelire.importer(fichiers)
    if (champ) champ.value = ''
  }

  function deposer(e: DragEvent) {
    e.preventDefault()
    survol = false
    if (!tirelire.envoi) envoyer(e.dataTransfer?.files)
  }

  async function supprimer(r: Releve) {
    const ok = await confirmer({
      titre: `Supprimer le relevé n° ${r.number} de ${r.year} ?`,
      message: `${pluriel(r.operations, 'opération sera supprimée', 'opérations seront supprimées')} avec lui. Le même PDF pourra être importé de nouveau.`,
      action: 'Supprimer',
      destructif: true,
    })
    if (!ok) return
    suppression = r.id
    try { await tirelire.supprimer(r.id) } finally { suppression = null }
  }
</script>

<section class="tir-depot" class:tir-depot--survol={survol} aria-label="Dépôt de fichiers" role="group"
  ondragover={(e) => { e.preventDefault(); survol = true }} ondragleave={() => (survol = false)} ondrop={deposer}>
  <span class="tir-pas tir-pas--grand tir-pas--rose"><Icon name="envoie" size={20} /></span>
  <h2 class="tir-h2">Déposer des relevés PDF</h2>
  <p class="tir-note">Plusieurs fichiers à la fois, 8 Mo au plus chacun. Les doublons sont détectés, rien n’est importé deux fois.</p>
  <button class="tir-btn tir-btn--plein" disabled={tirelire.envoi} onclick={() => champ?.click()}>{tirelire.envoi ? 'Envoi en cours…' : 'Choisir des fichiers'}</button>
  <input bind:this={champ} class="v-sr" type="file" accept="application/pdf,.pdf" multiple tabindex="-1" aria-label="Relevés PDF à importer" onchange={(e) => envoyer(e.currentTarget.files)} />
</section>

{#if tirelire.envoi}
  <div class="tir-etat" role="status"><Loader text="Envoi et contrôle des relevés…" /></div>
{/if}

{#if issues.length}
  <div class="tir-ligne tir-ligne--ecart" style:padding-top="8px"><h2 class="tir-h2">{pluriel(issues.length, 'fichier traité', 'fichiers traités')}</h2></div>
  <ul class="tir-ligne tir-ligne--plis" aria-label="Bilan de l’import">
    {#if b.importes}<li class="tir-etq tir-etq--ok"><Icon name="ok" size={14} />{pluriel(b.importes, 'importé', 'importés')}</li>{/if}
    {#if b.deja}<li class="tir-etq tir-etq--info">{b.deja} déjà {b.deja > 1 ? 'importés' : 'importé'}</li>{/if}
    {#if b.refuses}<li class="tir-etq tir-etq--danger"><Icon name="alerte" size={14} />{pluriel(b.refuses, 'refusé', 'refusés')}</li>{/if}
  </ul>
  {#each issues as issue (issue)}
    <TirelireFichier {issue} occupe={tirelire.envoi} onretry={tirelire.peutReessayer(issue) ? () => void tirelire.reessayer(issue) : undefined} />
  {/each}
  {#if b.importes}<a class="tir-btn tir-btn--rose tir-btn--large" href={hrefs.tirelire('operations')}>Voir {b.operations > 1 ? `les ${pluriel(b.operations, '', 'nouvelles opérations')}` : 'les opérations'}</a>{/if}
{/if}

<section class="tir-carte" aria-label="Relevés importés">
  <div class="tir-ligne tir-ligne--ecart"><h2 class="tir-h2">Relevés importés</h2>{#if tirelire.releves}<span class="tir-note">{tirelire.releves.length}</span>{/if}</div>
  {#if tirelire.erreurSuppression}<p class="tir-alerte" role="alert"><Icon name="alerte" size={18} />{tirelire.erreurSuppression}</p>{/if}
  {#if !tirelire.releves}
    {#if tirelire.erreur}
      <p class="tir-alerte" role="alert"><Icon name="alerte" size={18} />{tirelire.erreur}</p>
      <button class="tir-btn" onclick={() => void tirelire.chargerReleves()}>Réessayer</button>
    {:else}
      <div class="tir-etat"><Loader /></div>
    {/if}
  {:else if !tirelire.releves.length}
    <p class="tir-note tir-note--texte">Aucun relevé importé pour l’instant.</p>
  {:else}
    <ul class="tir-liste">
      {#each tirelire.releves as r (r.id)}
        <li>
          <div class="tir-li">
            <span class="tir-pas"><Icon name="file" size={18} /></span>
            <div class="tir-col tir-col--colle tir-grandit">
              <span class="tir-h3">{nomReleve(r)}</span>
              <span class="tir-note">{periode(r.from, r.to)} · {pluriel(r.operations, 'opération', 'opérations')}</span>
              <span class="tir-note">Importé le {instant(r.imported_at)}</span>
            </div>
            <button class="tir-ib tir-ib--danger" disabled={suppression !== null} aria-label="Supprimer le relevé n° {r.number} de {r.year}" title="Supprimer ce relevé" onclick={() => void supprimer(r)}><Icon name="corbeille" size={20} /></button>
          </div>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<a class="tir-btn tir-btn--large" href={hrefs.tirelire()}>Retour à l’accueil</a>
