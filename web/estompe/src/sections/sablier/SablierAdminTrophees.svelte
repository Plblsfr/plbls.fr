<script lang="ts">
  import { onMount } from 'svelte'
  import Icon, { traceIcone, type IconName } from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import { hrefs } from '../../lib/route.svelte'
  import { notify } from '../../lib/ui'
  import { ligneTrophee, resumeTrophees } from './admin'
  import { messageDe } from './admin.svelte'
  import { nombre } from './formats'
  import { adminApi, type Catalogue, type TropheeAdmin } from './sablier'
  import SablierErreur from './SablierErreur.svelte'

  let trophees = $state.raw<TropheeAdmin[] | null>(null)
  let catalogue = $state.raw<Catalogue | null>(null)
  let erreur = $state('')
  let envoi = $state<number | null>(null)

  async function charger() {
    erreur = ''
    try { [trophees, catalogue] = await Promise.all([adminApi.trophees(), adminApi.catalogue()]) } catch (e) { erreur = messageDe(e) }
  }
  onMount(() => { void charger() })

  const icone = (nom: string): IconName => (traceIcone(nom as IconName) ? (nom as IconName) : 'trophee')

  async function activer(t: TropheeAdmin, actif: boolean, champ: HTMLInputElement) {
    envoi = t.id
    try {
      const maj = await adminApi.modifier(t.id, { active: actif })
      trophees = (trophees ?? []).map((x) => (x.id === t.id ? maj : x))
    } catch (e) {
      champ.checked = !actif
      notify.error(e, 'Le trophée n’a pas pu être modifié')
    } finally {
      envoi = null
    }
  }
</script>

<section class="sab-sec" aria-label="Configuration des trophées">
  {#if erreur && !trophees}
    <SablierErreur message={erreur} onretry={() => void charger()} />
  {:else if !trophees}
    <div class="sab-attente"><Loader text="Ouverture des trophées…" /></div>
  {:else}
    <div class="sab-tete">
      <p class="sab-note">{resumeTrophees(trophees)} Un trophée désactivé n’est plus affiché et revient s’il est réactivé.</p>
      <a class="plbls-button" href={hrefs.stats('admin', 'trophees', 'nouveau')}><Icon name="plus" size={16} />Nouveau trophée</a>
    </div>
    <div class="sab-card sab-card--tableau"><div class="sab-scroll">
      <table class="sab-table">
        <thead><tr><th scope="col">Trophée</th><th scope="col">Type</th><th scope="col">Stat</th><th scope="col">Période</th><th scope="col">Seuils</th><th scope="col">Débloqué par</th><th scope="col">Actif</th><th scope="col"><span class="v-sr">Action</span></th></tr></thead>
        <tbody>
          {#each trophees as t (t.id)}
            {@const l = ligneTrophee(t, catalogue)}
            <tr data-trophee={t.key}>
              <th scope="row"><span class="sab-ligne sab-ligne--fixe"><Icon name={icone(t.icon)} size={18} />{t.name}{#if t.hidden}<span class="sab-etiquette sab-etiquette--rose">caché</span>{/if}</span></th>
              <td>{l.type}{#if l.code} <span class="sab-etiquette">codé</span>{/if}</td>
              <td>{l.stat}</td>
              <td>{l.periode}</td>
              <td>{l.seuils}</td>
              <td>{nombre(t.unlocked_by)}</td>
              <td><label class="plbls-toggle"><input class="plbls-toggle__input" type="checkbox" role="switch" aria-label="{t.name} actif" checked={t.active} disabled={envoi === t.id} onchange={(e) => void activer(t, e.currentTarget.checked, e.currentTarget)} /><span class="plbls-toggle__piste"></span></label></td>
              <td class="sab-table-action"><a class="plbls-button plbls-button--contour plbls-button--compact" href={hrefs.stats('admin', 'trophees', t.id)} aria-label="Modifier {t.name}">Modifier</a></td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div></div>
    <p class="sab-note">Les exploits « codés » ont une logique propre : l’admin les nomme, les décrit, choisit l’icône et le paramètre, les active ou les désactive, sans toucher à la condition.</p>
  {/if}
</section>
