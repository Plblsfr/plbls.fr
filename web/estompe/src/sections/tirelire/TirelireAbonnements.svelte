<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import { hrefs } from '../../lib/route.svelte'
  import { dateCourte, euros, moisLong, pluriel } from './formats'
  import { actifs, alertes, arretes, cadenceDe, couverture, ecartes, manque, PAR, phraseAlerte, type Groupe, type Recurrent, type Recurrents } from './habitudes'
  import TirelirePastille from './TirelirePastille.svelte'
  import { filtres, requeteDe } from './tirelire'
  import { tirelire } from './tirelire.svelte'

  let { recurrents: r }: { recurrents: Recurrents } = $props()

  let volet = $state<'tous' | Groupe>('tous')
  let info = $state('')

  const liste = $derived(actifs(r))
  const visibles = $derived(volet === 'tous' ? liste : liste.filter((x) => x.group === volet))
  const total = $derived(volet === 'tous' ? r.total : volet === 'charge' ? r.charges : r.subscriptions)
  const aVoir = $derived(alertes(r))
  const finis = $derived(arretes(r))
  const retires = $derived(ecartes(r))
  const besoin = $derived(manque(r))
  const lien = (x: Recurrent) => hrefs.tirelire('operations', undefined, requeteDe(filtres({ commercant: x.merchant_id, sens: 'debit' })))
  const nom = (x: Recurrent) => x.name ?? 'Commerçant sans nom'
  const occupe = (x: Recurrent) => tirelire.recurrentOccupe === x.id

  async function vu(x: Recurrent) {
    if (await tirelire.modifierRecurrent(x.id, { seen: true })) info = `Alerte de « ${nom(x)} » marquée comme vue.`
  }

  async function ecarter(x: Recurrent, ecarte: boolean) {
    if (await tirelire.modifierRecurrent(x.id, { dismissed: ecarte })) info = ecarte ? `« ${nom(x)} » écarté : il ne compte plus dans les totaux ni dans les alertes.` : `« ${nom(x)} » rétabli.`
  }
</script>

{#snippet marques(x: Recurrent)}
  {#each x.alerts as a (a.kind)}<span class="tir-etq" class:tir-etq--danger={a.kind === 'amount_changed'} class:tir-etq--rose={a.kind === 'new'}>{a.kind === 'new' ? 'nouveau' : 'montant changé'}{a.seen ? ' · vu' : ''}</span>{/each}
  {#if x.missed > 0 && x.status === 'active'}<span class="tir-etq tir-etq--attente">{pluriel(x.missed, 'échéance manquée', 'échéances manquées')}</span>{/if}
{/snippet}

<div class="tir-ev tir-abo">
  {#if !r.items.length}
    <section class="tir-carte tir-etat tir-ev-large" aria-label="Aucun récurrent détecté">
      <span class="tir-pas tir-pas--grand tir-pas--rose"><Icon name="actualise" size={22} /></span>
      <h2 class="tir-h2">{besoin ? 'Historique trop court pour repérer une cadence' : 'Aucun paiement récurrent repéré'}</h2>
      <p class="tir-note tir-note--texte">{couverture(r)}. Un abonnement se reconnaît à un même montant qui revient à intervalle régulier chez le même commerçant{besoin ? ` : avec ${r.statements > 1 ? 'ces relevés' : 'un seul relevé'}, rien ne peut encore se confirmer` : ''}.</p>
      {#if besoin}<p class="tir-note tir-note--texte">{besoin}</p>{/if}
      <a class="tir-btn tir-btn--plein" href={hrefs.tirelire('import')}>Importer d’autres relevés</a>
    </section>
  {:else}
    <div class="tir-g2 tir-abo-tuiles">
      <section class="tir-carte tir-carte--serre" aria-label="Coût mensuel"><span class="tir-note">Par mois</span><span class="tir-val tir-fixe">{euros(r.total.monthly)}</span><span class="tir-note">{pluriel(r.total.count, 'paiement récurrent', 'paiements récurrents')}</span></section>
      <section class="tir-carte tir-carte--serre" aria-label="Coût annuel"><span class="tir-note">Par an</span><span class="tir-val tir-fixe">{euros(r.total.yearly)}</span><span class="tir-note">au dernier montant de chacun</span></section>
    </div>
    <section class="tir-carte tir-carte--serre tir-abo-dont" aria-label="Répartition">
      <p class="tir-ligne tir-ligne--ecart tir-ligne--plis" style:gap="2px 12px"><span class="tir-note">Abonnements · {r.subscriptions.count}</span><span class="tir-fort tir-fixe">{euros(r.subscriptions.monthly)} par mois · {euros(r.subscriptions.yearly)} par an</span></p>
      <p class="tir-ligne tir-ligne--ecart tir-ligne--plis" style:gap="2px 12px"><span class="tir-note">Charges fixes (loyer, crédit, assurances, énergie…) · {r.charges.count}</span><span class="tir-fort tir-fixe">{euros(r.charges.monthly)} par mois · {euros(r.charges.yearly)} par an</span></p>
    </section>

    <section class="tir-col tir-abo-alertes" style:gap="10px" aria-label="Alertes">
      <h2 class="tir-h2">{aVoir.length ? pluriel(aVoir.length, 'alerte', 'alertes') : 'Alertes'}</h2>
      {#if tirelire.erreurRecurrents}<p class="tir-alerte" role="alert"><Icon name="alerte" size={18} />{tirelire.erreurRecurrents}</p>{/if}
      {#if info}<p class="tir-fait" role="status"><Icon name="ok" size={18} /><span class="tir-note tir-note--texte">{info}</span></p>{/if}
      {#each aVoir as a (a.recurrent.id + a.alerte.kind)}
        {@const x = a.recurrent}
        <article class="tir-carte" class:tir-carte--alerte={a.alerte.kind === 'amount_changed'} class:tir-carte--rose={a.alerte.kind === 'new'} aria-label="{a.alerte.kind === 'new' ? 'Nouvel abonnement' : 'Montant changé'} : {nom(x)}">
          <div class="tir-ligne tir-ligne--haut" style:gap="12px">
            <span class="tir-pas" class:tir-pas--danger={a.alerte.kind === 'amount_changed'}><Icon name={a.alerte.kind === 'new' ? 'plus' : 'alerte'} size={18} /></span>
            <span class="tir-col tir-col--serre tir-grandit"><span class="tir-etq" class:tir-etq--danger={a.alerte.kind === 'amount_changed'} style:align-self="flex-start">{a.alerte.kind === 'new' ? 'Nouvel abonnement' : 'Montant changé'}</span><span class="tir-h3">{nom(x)}</span></span>
            <span class="tir-col tir-col--colle" style:align-items="flex-end">
              {#if a.alerte.old_amount !== null}<span class="tir-note tir-fixe"><span class="tir-sr">ancien montant : </span><s>{euros(a.alerte.old_amount)}</s></span>{:else}<span class="tir-note">{PAR[x.cadence]}</span>{/if}
              <span class="tir-val tir-fixe"><span class="tir-sr">{a.alerte.old_amount !== null ? 'nouveau montant : ' : ''}</span>{euros(a.alerte.new_amount)}</span>
            </span>
          </div>
          <p class="tir-note tir-note--texte tir-note--clair">{phraseAlerte(a)}</p>
          <div class="tir-abo-actions">
            <button type="button" class="tir-btn tir-btn--plein" disabled={!!tirelire.recurrentOccupe} onclick={() => void vu(x)}>{occupe(x) ? 'Enregistrement…' : 'C’est noté'}</button>
            {#if a.alerte.kind === 'new'}
              <button type="button" class="tir-btn tir-btn--contour" disabled={!!tirelire.recurrentOccupe} onclick={() => void ecarter(x, true)}>Pas un abonnement</button>
            {:else}
              <a class="tir-btn tir-btn--contour" href={hrefs.tirelire('operations', a.alerte.operation_id)}>Voir l’opération</a>
            {/if}
          </div>
        </article>
      {:else}
        <div class="tir-carte tir-carte--creux"><p class="tir-note tir-note--texte"><Icon name="ok" size={16} /> Aucune alerte : ni nouvel abonnement, ni montant changé sur les 4 derniers mois de tes relevés.</p></div>
      {/each}
    </section>

    <section class="tir-carte tir-abo-liste" aria-label="Tous les récurrents">
      <div class="tir-ligne tir-ligne--ecart tir-ligne--plis"><h2 class="tir-h2">Tous les récurrents</h2><span class="tir-note">triés par coût annuel</span></div>
      <div class="tir-seg" role="group" aria-label="Filtrer les récurrents">
        <button type="button" aria-pressed={volet === 'tous'} onclick={() => (volet = 'tous')}>Tous · {r.total.count}</button>
        <button type="button" aria-pressed={volet === 'subscription'} onclick={() => (volet = 'subscription')}>Abonnements · {r.subscriptions.count}</button>
        <button type="button" aria-pressed={volet === 'charge'} onclick={() => (volet = 'charge')}>Charges · {r.charges.count}</button>
      </div>
      {#if visibles.length}
        <ul class="tir-liste">
          {#each visibles as x (x.id)}
            <li class="tir-abo-ligne">
              <a class="tir-li tir-grandit" href={lien(x)}>
                <span class="tir-col tir-col--serre tir-grandit">
                  <span class="tir-h3">{nom(x)}</span>
                  <span class="tir-note">{cadenceDe(x)} · {euros(x.amount)}</span>
                  <span class="tir-note">depuis {moisLong(x.since)}{x.next_date ? ` · prochaine vers le ${dateCourte(x.next_date)}` : ''}</span>
                  <span class="tir-sous"><TirelirePastille categorie={tirelire.categorie(x.category_id)} />{@render marques(x)}</span>
                </span>
                <span class="tir-col tir-col--colle" style:align-items="flex-end"><span class="tir-fort tir-fixe">{euros(x.yearly)}</span><span class="tir-note">par an</span></span>
              </a>
              <button type="button" class="tir-ib" disabled={!!tirelire.recurrentOccupe} aria-label="Écarter {nom(x)} : ce n’est pas un abonnement" title="Ce n’est pas un abonnement" onclick={() => void ecarter(x, true)}><Icon name="x" size={18} /></button>
            </li>
          {/each}
        </ul>
        <div class="tir-ligne tir-ligne--ecart tir-separe"><span class="tir-h3">Total</span><span class="tir-col tir-col--colle" style:align-items="flex-end"><span class="tir-fort tir-fixe">{euros(total.yearly)} par an</span><span class="tir-note">{euros(total.monthly)} par mois</span></span></div>
      {:else}
        <p class="tir-note tir-note--texte">{volet === 'charge' ? 'Aucune charge fixe récurrente repérée.' : volet === 'subscription' ? 'Aucun abonnement repéré.' : 'Tous les récurrents repérés sont arrêtés ou écartés.'}</p>
      {/if}
      <p class="tir-note">La croix écarte un faux abonnement : il sort des totaux et des alertes, et tu peux le rétablir plus bas. Coût annuel : dernier montant × nombre d’échéances par an.</p>
    </section>

    {#if finis.length}
      <section class="tir-carte tir-abo-finis" aria-label="Arrêtés">
        <div class="tir-col tir-col--serre"><h2 class="tir-h2">Arrêtés</h2><span class="tir-note">les échéances attendues ne sont plus venues</span></div>
        <ul class="tir-liste">
          {#each finis as x (x.id)}
            <li><a class="tir-li" href={lien(x)}><span class="tir-col tir-col--colle tir-grandit"><span class="tir-h3">{nom(x)}</span><span class="tir-note">{cadenceDe(x)} · {euros(x.amount)} · dernière échéance le {dateCourte(x.last_date)}</span></span><span class="tir-etq">arrêté</span></a></li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if retires.length}
      <section class="tir-carte tir-abo-ecartes" aria-label="Écartés">
        <div class="tir-col tir-col--serre"><h2 class="tir-h2">Écartés</h2><span class="tir-note">tu as dit que ce ne sont pas des abonnements</span></div>
        <ul class="tir-liste">
          {#each retires as x (x.id)}
            <li class="tir-li"><span class="tir-col tir-col--colle tir-grandit"><span class="tir-h3">{nom(x)}</span><span class="tir-note">{cadenceDe(x)} · {euros(x.amount)}</span></span><button type="button" class="tir-btn tir-btn--contour" disabled={!!tirelire.recurrentOccupe} aria-label="Rétablir {nom(x)}" onclick={() => void ecarter(x, false)}>Rétablir</button></li>
          {/each}
        </ul>
      </section>
    {/if}

    <section class="tir-carte tir-carte--creux tir-ev-large" aria-label="Méthode">
      <p class="tir-note tir-note--texte">{couverture(r)}{r.as_of ? `, jusqu’au ${dateCourte(r.as_of)}` : ''}. Un récurrent est un même commerçant, un montant proche (5 % ou 1 €) et une cadence régulière : chaque semaine (à 2 jours près, 4 fois au moins), chaque mois (5 jours, 3 fois, 2 pour un prélèvement), chaque trimestre (7 jours) ou chaque année (15 jours, 2 fois). Il est dit arrêté après 2 échéances manquées (1 pour un trimestriel ou un annuel). L’épargne et les virements entre tes comptes ne comptent pas.</p>
      {#if besoin}<p class="tir-note tir-note--texte">Ton historique est encore court. {besoin}</p>{/if}
    </section>
  {/if}
</div>
