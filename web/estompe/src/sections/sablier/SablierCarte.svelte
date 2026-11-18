<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import { majuscule, nomMois, versDate } from './formats'
  import { enCours, enPeriode } from './periodes'
  import type { Periode } from './sablier'
  import type { Ailleurs } from './sablier.svelte'
  import { ICONE_DOMAINE, NOM_DOMAINE, SANS_ACCES, TEXTE_VIDE, type Carte } from './stats'

  let { carte, periode, date, aujourdhui, ailleurs = null, onaller }: { carte: Carte; periode: Periode; date: string; aujourdhui: string; ailleurs?: Ailleurs | null; onaller?: (a: Ailleurs) => void } = $props()
  const nom = $derived(NOM_DOMAINE[carte.domaine])
  const refus = $derived(SANS_ACCES[carte.domaine])
  const quand = $derived(periode === 'all' ? '' : enCours(periode, date, aujourdhui) ? (periode === 'week' ? 'cette semaine' : periode === 'month' ? 'ce mois-ci' : 'cette année') : enPeriode(periode, date))
  const titreVide = $derived(periode === 'all' ? TEXTE_VIDE[carte.domaine].jamais : `${TEXTE_VIDE[carte.domaine].rien} ${quand}`)
  const renvoi = $derived.by(() => {
    if (!ailleurs) return null
    if (ailleurs.periode === 'month') return { phrase: `${majuscule(nomMois(ailleurs.date))} a de quoi raconter.`, bouton: `Voir ${nomMois(ailleurs.date)}` }
    if (ailleurs.periode === 'year') return { phrase: `L’année ${versDate(ailleurs.date).getUTCFullYear()} a de quoi raconter.`, bouton: `Voir ${versDate(ailleurs.date).getUTCFullYear()}` }
    return { phrase: 'Il y a de quoi raconter depuis le début.', bouton: 'Voir tout' }
  })
</script>

{#if carte.etat === 'sans-acces' && refus}
  <div class="sab-card sab-card--creux" data-domaine={carte.domaine} data-etat="sans-acces">
    <div class="sab-card-tete"><Icon name="cadenas" size={18} />{nom}</div>
    <p class="sab-h3">{refus.titre}</p>
    <p class="sab-note">{refus.texte}</p>
  </div>
{:else if carte.etat === 'vide'}
  <div class="sab-card sab-card--creux" data-domaine={carte.domaine} data-etat="vide">
    <div class="sab-card-tete"><Icon name={ICONE_DOMAINE[carte.domaine]} size={18} />{nom}</div>
    <p class="sab-h3">{titreVide}</p>
    {#if renvoi && ailleurs}
      <p class="sab-note">{renvoi.phrase}</p>
      <div class="sab-ligne"><button class="plbls-button plbls-button--contour plbls-button--compact" type="button" onclick={() => onaller?.(ailleurs)}>{renvoi.bouton}</button></div>
    {/if}
  </div>
{:else if carte.etat === 'pleine'}
  <div class="sab-card" data-domaine={carte.domaine} data-etat="pleine">
    <div class="sab-card-tete"><Icon name={ICONE_DOMAINE[carte.domaine]} size={18} />{nom}</div>
    {#if carte.chiffre}<p class="sab-num">{carte.chiffre.valeur} <small>{carte.chiffre.unite}</small></p>{/if}
    {#if carte.barres}
      <div class="sab-barres" role="img" aria-label={carte.barres.description}>
        {#each carte.barres.liste as b, i (i)}<i class:off={!b.actif} style:height="{b.hauteur}%"></i>{/each}
      </div>
    {/if}
    {#if carte.tuiles.length}
      <div class="sab-g sab-g--serre sab-g{Math.min(3, Math.max(2, carte.tuiles.length))}">
        {#each carte.tuiles as t (t.label)}<div class="sab-tile"><span>{t.label}</span><b>{t.valeur}</b></div>{/each}
      </div>
    {/if}
    {#if carte.parts.length}
      <ul class="sab-reps">
        {#each carte.parts as p, i (i)}<li class="sab-rep"><span>{p.label}</span><div class="sab-prog" aria-hidden="true"><i style:width="{p.part}%"></i></div><b>{p.valeur}</b></li>{/each}
      </ul>
    {/if}
    {#if carte.lignes.length}
      <dl class="sab-kvs">
        {#each carte.lignes as l (l.label)}<div class="sab-kv"><dt><span>{l.label}</span></dt><dd><b>{l.valeur}</b></dd></div>{/each}
      </dl>
    {/if}
    {#if carte.etiquettes.length}
      <div class="sab-ligne">{#each carte.etiquettes as e (e)}<span class="sab-etiquette">{e}</span>{/each}</div>
    {/if}
    {#if carte.note}<p class="sab-note">{carte.note}</p>{/if}
  </div>
{/if}
