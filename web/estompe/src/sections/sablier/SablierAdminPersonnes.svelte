<script lang="ts">
  import { onMount } from 'svelte'
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import { hrefs } from '../../lib/route.svelte'
  import { app } from '../../lib/store.svelte'
  import { notify } from '../../lib/ui'
  import { avecCompte, filtrerPersonnes, nomPersonne, sansCompte } from './admin'
  import { admin } from './admin.svelte'
  import { confirmerEffacementDe } from './effacement'
  import { duree, ilYA, jourLocal, jourLong } from './formats'
  import type { Personne } from './sablier'
  import SablierErreur from './SablierErreur.svelte'

  let recherche = $state('')
  let enCours = $state('')
  onMount(() => { void admin.charger() })

  const aujourdhui = jourLocal()
  const tous = $derived(admin.personnes ? filtrerPersonnes(admin.personnes.people, recherche) : [])
  const connus = $derived(avecCompte(tous))
  const orphelins = $derived(sansCompte(tous))
  const moi = $derived(app.sablierMe?.sub)

  async function effacer(p: Personne) {
    if (enCours || !(await confirmerEffacementDe(nomPersonne(p)))) return
    enCours = p.sub
    try {
      await admin.effacer(p.sub)
      notify.ok(`Les données de ${nomPersonne(p)} sont effacées.`)
    } catch (e) {
      notify.error(e, 'Les données n’ont pas pu être effacées')
    } finally {
      enCours = ''
    }
  }
</script>

{#if admin.erreur && !admin.personnes}
  <SablierErreur message={admin.erreur} onretry={() => void admin.charger()} />
{:else if !admin.personnes}
  <div class="sab-attente"><Loader text="Ouverture des personnes…" /></div>
{:else}
  <section class="sab-sec" aria-label="Personnes">
    <div class="sab-alerte"><Icon name="oeil" size={18} /><p>Ouvrir les stats d’une personne est noté au journal. Elle voit ton nom et la date dans « Partage et données ».</p></div>
    <div class="plbls-field">
      <label class="plbls-field__label" for="sab-recherche">Chercher une personne</label>
      <input id="sab-recherche" class="plbls-input" type="search" placeholder="Nom ou identifiant" bind:value={recherche} />
    </div>
    {#if connus.length}
      <div class="sab-card sab-card--tableau"><div class="sab-scroll">
        <table class="sab-table">
          <thead><tr><th scope="col">Personne</th><th scope="col">Dernière activité</th><th scope="col">Temps actif</th><th scope="col">Premier jour</th><th scope="col"><span class="v-sr">Action</span></th></tr></thead>
          <tbody>
            {#each connus as p (p.sub)}
              <tr>
                <th scope="row">{nomPersonne(p)}{#if p.sub === moi}<span class="sab-etiquette sab-etiquette--marge">toi</span>{/if}</th>
                <td>{ilYA(p.last_day, aujourdhui)}</td>
                <td>{p.active_seconds_total > 0 ? duree(p.active_seconds_total) : '—'}</td>
                <td>{p.first_day ? jourLong(p.first_day) : '—'}</td>
                <td class="sab-table-action"><a class="plbls-button plbls-button--contour plbls-button--compact" href={hrefs.stats('admin', 'personnes', p.sub)} aria-label="Voir ses stats, {nomPersonne(p)}">Voir ses stats</a></td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div></div>
    {:else}
      <p class="sab-note">{recherche.trim() ? 'Personne ne correspond à cette recherche.' : 'Aucune personne avec un compte n’a de stats.'}</p>
    {/if}
  </section>
  {#if orphelins.length || admin.personnes.directory === 'local'}
    <section class="sab-sec" aria-label="Personnes sans compte">
      <div class="sab-tete"><h2 class="sab-h2">Personnes sans compte</h2><p class="sab-note">Des stats existent, mais le compte n’est plus dans l’annuaire.</p></div>
      {#if admin.personnes.directory === 'local'}
        <div class="sab-alerte" role="note"><Icon name="alerte" size={18} /><p>L’annuaire d’aegis ne répond pas : cette liste est approximative. Un compte déjà vu connecté puis supprimé n’y figure pas.</p></div>
      {/if}
      {#if orphelins.length}
        <ul class="sab-card sab-card--serre">
          {#each orphelins as p (p.sub)}
            <li class="sab-reg sab-reg--fixe sab-reg--souple">
              <span class="sab-col sab-grow" style:gap="0"><b>{nomPersonne(p)}</b><span class="sab-note">{p.first_day && p.last_day ? `Stats du ${jourLong(p.first_day)} au ${jourLong(p.last_day)} · dernière activité ${ilYA(p.last_day, aujourdhui)}` : 'Aucune journée d’activité'}</span></span>
              <a class="plbls-button plbls-button--contour plbls-button--compact" href={hrefs.stats('admin', 'personnes', p.sub)} aria-label="Voir ses stats, {nomPersonne(p)}">Voir ses stats</a>
              <button class="plbls-button plbls-button--danger plbls-button--compact" type="button" disabled={enCours === p.sub} onclick={() => void effacer(p)} aria-label="Effacer ses données…, {nomPersonne(p)}">Effacer ses données…</button>
            </li>
          {/each}
        </ul>
      {:else}
        <p class="sab-note">Aucune personne sans compte{recherche.trim() ? ' pour cette recherche' : ''}.</p>
      {/if}
    </section>
  {/if}
{/if}
