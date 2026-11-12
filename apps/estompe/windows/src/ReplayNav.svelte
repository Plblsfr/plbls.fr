<script lang="ts">
  import Icon from '../../../../web/estompe/src/components/Icon.svelte'
  import { hrefs, router } from '../../../../web/estompe/src/lib/route.svelte'
  import { openSettings } from '../../../../web/estompe/src/lib/settings.svelte'
  import { ESPACE, jeuxDe, PAGE_REGLAGES, partUtilisee, taille } from './replay'
  import { ouvrirDossier, replay } from './replay.svelte'

  const section = $derived(router.current.name === 'space' ? router.current.section : undefined)
  const jeux = $derived(jeuxDe(replay.clips))
</script>

<div class="side-head"><h2>Clips</h2><span class="plbls-badge">bêta</span></div>
<div class="side-list">
  <a class="nav-item" class:active={!section} href={hrefs.space(ESPACE)} aria-current={!section ? 'page' : undefined}>
    <Icon name="clap" size={18} /><span class="grow truncate">Tous les clips</span>{#if replay.clips.length}<span class="v-mono v-mute">{replay.clips.length}</span>{/if}
  </a>
  {#if jeux.length > 1}
    <div class="side-title">Jeux</div>
    {#each jeux as j (j.jeu)}
      <a class="nav-item" class:active={section === j.jeu} href={hrefs.space(ESPACE, encodeURIComponent(j.jeu))} aria-current={section === j.jeu ? 'page' : undefined}>
        <Icon name="cible" size={18} /><span class="grow truncate">{j.jeu}</span><span class="v-mono v-mute">{j.nombre}</span>
      </a>
    {/each}
  {/if}
  {#if replay.stockage}
    {@const s = replay.stockage}
    <div class="plbls-progress rn-usage">
      <div class="plbls-progress__entete"><span>Espace utilisé</span><span>{taille(s.utiliseOctets)} / {taille(s.quotaOctets)}</span></div>
      <div class="plbls-progress__piste" role="progressbar" aria-label="Espace utilisé" aria-valuemin="0" aria-valuemax="100" aria-valuenow={partUtilisee(s)}><div class="plbls-progress__remplissage" style:width="{partUtilisee(s)}%"></div></div>
    </div>
  {/if}
  <button class="nav-item" onclick={ouvrirDossier}><Icon name="external" size={18} /><span class="grow truncate">Ouvrir le dossier</span></button>
  <button class="nav-item" onclick={() => openSettings(PAGE_REGLAGES)}><Icon name="reglages" size={18} /><span class="grow truncate">Réglages de l’instant replay</span></button>
</div>

<style>
  .side-head { gap: 8px; }
  .rn-usage { margin: var(--espace-4) var(--espace-2) var(--espace-2); }
  button.nav-item { text-align: left; cursor: pointer; }
</style>
