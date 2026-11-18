<script lang="ts">
  import { onMount } from 'svelte'
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import { openProfile } from '../../lib/profile.svelte'
  import { app } from '../../lib/store.svelte'
  import { notify } from '../../lib/ui'
  import type { Domaine, StatPartagee } from './sablier'
  import SablierErreur from './SablierErreur.svelte'
  import SablierMedaille from './SablierMedaille.svelte'
  import { sablier } from './sablier.svelte'

  const APERCU = 6
  const GROUPES: { domaine: Domaine; titre: string; note: string }[] = [
    { domaine: 'usage', titre: 'Utilisation et séries', note: '' },
    { domaine: 'messaging', titre: 'Messagerie et vocal', note: '' },
    { domaine: 'entracte', titre: 'Entracte', note: 'Vu seulement par qui a aussi accès à Entracte.' },
    { domaine: 'sillon', titre: 'Sillon', note: 'Vu seulement par qui a aussi accès à Sillon.' },
  ]
  let tous = $state(false)
  let envoi = $state('')
  onMount(() => { void sablier.chargerPartage() })

  const groupes = $derived(GROUPES.map((g) => ({ ...g, stats: (sablier.partage?.stats ?? []).filter((s: StatPartagee) => s.domain === g.domaine) })).filter((g) => g.stats.length))
  const trophees = $derived(sablier.partage?.trophies ?? [])
  const visibles = $derived(tous ? trophees : trophees.slice(0, APERCU))
  const affiches = $derived(trophees.filter((t) => t.shared).length)

  async function regler(cle: string, choix: { stats?: Record<string, boolean>; trophies?: Record<string, boolean> }, champ: HTMLInputElement) {
    envoi = cle
    try {
      await sablier.partager(choix)
    } catch (e) {
      champ.checked = !champ.checked
      notify.error(e, 'Le réglage n’a pas pu être enregistré')
    } finally {
      envoi = ''
    }
  }
</script>

<div class="film-page sab">
  <div class="sab-page">
    <header class="sab-sec">
      <h1 class="sab-h1">Partage et <span class="sab-hl">données</span></h1>
      <p class="sab-note">Tout est privé par défaut. Ce que tu actives ici apparaît sur ton profil, pour le mois en cours et le total, aux personnes qui peuvent déjà l’ouvrir.</p>
    </header>
    {#if sablier.erreurPartage && !sablier.partage}
      <SablierErreur message={sablier.erreurPartage} onretry={() => void sablier.chargerPartage()} />
    {:else if !sablier.partage}
      <div class="sab-attente"><Loader text="Ouverture des réglages…" /></div>
    {:else}
      <section class="sab-sec" aria-label="Stats affichées sur le profil">
        <div class="sab-tete">
          <h2 class="sab-h2">Stats sur mon profil</h2>
          {#if app.meId}<button class="plbls-button plbls-button--contour plbls-button--compact" type="button" onclick={() => openProfile(app.meId)}><Icon name="oeil" size={16} />Voir mon profil</button>{/if}
        </div>
        <div class="sab-g sab-g2">
          {#each groupes as g (g.domaine)}
            <div class="sab-card sab-card--serre" data-groupe={g.domaine}>
              <h3 class="sab-h3">{g.titre}</h3>
              {#if g.note}<p class="sab-note">{g.note}</p>{/if}
              <div style:padding-top="6px">
                {#each g.stats as s (s.key)}
                  <label class="sab-reg plbls-toggle">
                    <span class="sab-grow plbls-toggle__texte">{s.label}</span>
                    <input class="plbls-toggle__input" type="checkbox" role="switch" checked={s.shared} disabled={envoi === s.key} onchange={(e) => void regler(s.key, { stats: { [s.key]: e.currentTarget.checked } }, e.currentTarget)} />
                    <span class="plbls-toggle__piste"></span>
                  </label>
                {/each}
              </div>
            </div>
          {/each}
        </div>
        <p class="sab-note">Les records datés (plus longue session, journée la plus bavarde…), le détail par salon et par plateforme ne peuvent pas être rendus publics.</p>
      </section>

      <section class="sab-sec" aria-label="Trophées affichés sur le profil">
        <div class="sab-tete">
          <h2 class="sab-h2">Trophées sur mon profil</h2>
          <p class="sab-note">{trophees.length ? `${affiches} ${affiches >= 2 ? 'affichés' : 'affiché'} sur ${trophees.length} ${trophees.length >= 2 ? 'débloqués' : 'débloqué'}. Un nouveau trophée est privé.` : 'Aucun trophée débloqué pour l’instant. Un nouveau trophée est privé.'}</p>
        </div>
        {#if trophees.length}
          <div class="sab-card sab-card--serre">
            {#each visibles as t (t.id)}
              <label class="sab-reg plbls-toggle">
                <SablierMedaille icone={t.icon} taille="s" etat={t.shared ? 'acquis' : 'cours'} />
                <span class="sab-grow plbls-toggle__texte"><b>{t.name}</b></span>
                <input class="plbls-toggle__input" type="checkbox" role="switch" checked={t.shared} disabled={envoi === `t${t.id}`} onchange={(e) => void regler(`t${t.id}`, { trophies: { [t.id]: e.currentTarget.checked } }, e.currentTarget)} />
                <span class="plbls-toggle__piste"></span>
              </label>
            {/each}
          </div>
          {#if trophees.length > APERCU && !tous}
            <div class="sab-ligne"><button class="plbls-button plbls-button--contour plbls-button--compact" type="button" onclick={() => (tous = true)}>Les {trophees.length - APERCU} autres</button></div>
          {/if}
        {/if}
      </section>
    {/if}
  </div>
</div>
