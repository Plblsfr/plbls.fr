<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import { hrefs } from '../../lib/route.svelte'
  import { dateCourte, euros, periode, pluriel } from './formats'
  import { classes, FENETRES_HABITUDES, forts, JOURS, JOURS_COURTS, lienOperations, nomFenetreHabitudes, nomSemaine, nomSemaineLong, phraseJours, phraseSemaines, raisonDe, type Habitudes, type Recurrents, type Tri } from './habitudes'
  import { eurosRonds, MODES, unite } from './stats'
  import TirelireEcart from './TirelireEcart.svelte'
  import { teinte } from './tirelire'
  import { tirelire } from './tirelire.svelte'

  let { habitudes: h, recurrents }: { habitudes: Habitudes; recurrents: Recurrents | null } = $props()

  let tri = $state<Tri>('montant')

  const aJour = $derived(h.mode === tirelire.mode && h.window === tirelire.fenetreHabitudes)
  const lignes = $derived(classes(h, tri))
  const maximum = $derived(Math.max(1, ...lignes.map((m) => (tri === 'montant' ? m.amount : m.count))))
  const largeur = (v: number) => `${Math.max(0, Math.min(100, (v / maximum) * 100))}%`
  const jours = $derived(h.weekdays.buckets)
  const hautJour = $derived(Math.max(1, ...jours.map((t) => t.amount)))
  const marques = $derived(forts(h.weekdays))
  const semaines = $derived(h.weeks.buckets.filter((t) => t.days > 0 || t.amount !== 0))
  const hauteSemaine = $derived(Math.max(1, ...semaines.map((t) => t.amount)))
  const forte = $derived(semaines.length ? semaines.reduce((a, t) => (t.amount > a.amount ? t : a)).rank : 0)
  const etendue = $derived(h.current ? (h.mode === 'calendar' ? periode(h.current.from, h.known_through ?? h.current.to) : periode(h.current.from, h.current.to)) : '')
  const avant = $derived(h.previous ? (h.previous.periods === 1 ? `${unite(h.mode)} précédent` : `${h.previous.periods} ${unite(h.mode, h.previous.periods)} précédents`) : '')
  const categorie = (id: number | null) => tirelire.categorie(id)
  const passages = (n: number) => pluriel(n, 'passage', 'passages')
</script>

<div class="tir-ev tir-hab" style:opacity={aJour ? undefined : 0.6} aria-busy={!aJour}>
  <section class="tir-carte tir-carte--serre tir-ev-reglages" aria-label="Fenêtre et découpage">
    <div class="tir-seg tir-seg--dense" role="group" aria-label="Fenêtre étudiée">
      {#each FENETRES_HABITUDES as f (f)}<button type="button" aria-pressed={tirelire.fenetreHabitudes === f} onclick={() => tirelire.changerFenetreHabitudes(f)}>{nomFenetreHabitudes(f, tirelire.mode)}</button>{/each}
    </div>
    <div class="tir-seg" role="group" aria-label="Découpage des périodes">
      {#each MODES as m (m.id)}<button type="button" aria-pressed={tirelire.mode === m.id} onclick={() => tirelire.changerMode(m.id)}>{m.label}</button>{/each}
    </div>
    <p class="tir-note">{etendue}{h.current ? ` · ${pluriel(h.covered_days, 'jour couvert', 'jours couverts')} par tes relevés` : ''}</p>
  </section>

  <a class="tir-bandeau tir-hab-acces" class:tir-bandeau--calme={!recurrents?.alerts_unseen} href={hrefs.tirelire('habitudes', 'abonnements')}>
    <span class="tir-pas" class:tir-pas--rose={!recurrents?.alerts_unseen}><Icon name="actualise" size={18} /></span>
    <span class="tir-col tir-col--colle tir-grandit">
      <span class="tir-h3">Abonnements et prélèvements</span>
      <span class="tir-note">{#if !recurrents}coût mensuel, prochaines échéances, alertes{:else if recurrents.total.count}{euros(recurrents.total.monthly)} par mois · {pluriel(recurrents.total.count, 'récurrent', 'récurrents')}{:else}aucun récurrent détecté pour l’instant{/if}</span>
    </span>
    {#if recurrents?.alerts_unseen}<span class="tir-etq tir-etq--fort">{pluriel(recurrents.alerts_unseen, 'alerte', 'alertes')}</span>{/if}
    <Icon name="droite" size={18} />
  </a>

  <section class="tir-carte tir-hab-top" aria-label="Top commerçants">
    <div class="tir-ligne tir-ligne--ecart tir-ligne--plis">
      <div class="tir-col tir-col--serre"><h2 class="tir-h2">Top commerçants</h2><span class="tir-note">dépenses variables{avant ? ` · évolution vs ${avant}` : ''}</span></div>
      {#if h.merchants.length > 1}
        <div class="tir-seg tir-hab-tri" role="group" aria-label="Classement des commerçants">
          <button type="button" aria-pressed={tri === 'montant'} onclick={() => (tri = 'montant')}>Par montant</button>
          <button type="button" aria-pressed={tri === 'nombre'} onclick={() => (tri = 'nombre')}>Par nombre</button>
        </div>
      {/if}
    </div>
    {#if lignes.length}
      <ol class="tir-liste">
        {#each lignes as m, i (m.merchant_id)}
          {@const c = categorie(m.category_id)}
          <li>
            <a class="tir-li tir-hab-ligne" href={hrefs.tirelire('operations', undefined, lienOperations(h, m.merchant_id))}>
              <span class="tir-pas" aria-hidden="true">{i + 1}</span>
              <span class="tir-col tir-col--serre tir-grandit">
                <span class="tir-ligne tir-ligne--ecart" style:gap="8px"><span class="tir-h3 truncate"><span class="tir-sr">{i + 1}. </span>{m.name ?? 'Commerçant sans nom'}</span><span class="tir-fort tir-fixe">{tri === 'montant' ? euros(m.amount) : passages(m.count)}</span></span>
                <span class="tir-hb" aria-hidden="true"><span class="tir-hb-plein tir-pt--{teinte(c)}" style:width={largeur(tri === 'montant' ? m.amount : m.count)}></span></span>
                <span class="tir-ligne tir-ligne--ecart tir-ligne--plis" style:gap="2px 8px">
                  <span class="tir-note">{tri === 'montant' ? passages(m.count) : euros(m.amount)}{m.average !== null ? ` · panier ${euros(m.average)}` : ''}{c ? ` · ${c.name}` : ' · à classer'}</span>
                  {#if m.previous}<TirelireEcart ecart={m.previous} />{/if}
                </span>
              </span>
            </a>
          </li>
        {/each}
      </ol>
      <a class="tir-btn tir-btn--large" href={hrefs.tirelire('operations', undefined, lienOperations(h))}>{h.merchants_total > lignes.length ? `Les ${lignes.length} premiers sur ${h.merchants_total} · toutes les opérations` : 'Toutes les opérations de la période'}<Icon name="droite" size={16} /></a>
    {:else}
      <p class="tir-note tir-note--texte">Aucune dépense variable chez un commerçant sur cette fenêtre.</p>
    {/if}
  </section>

  <section class="tir-carte tir-hab-petits" aria-label="Petits achats répétés">
    <div class="tir-col tir-col--serre"><h2 class="tir-h2">Petits achats répétés</h2><span class="tir-note">moins de {eurosRonds(h.small.threshold)}, au moins {h.small.min_repeats} fois chez le même commerçant</span></div>
    {#if h.small.count}
      <div class="tir-hab-tuiles">
        <div class="tir-carte tir-carte--creux tir-carte--serre"><span class="tir-note">Achats</span><span class="tir-val tir-fixe">{h.small.count}</span></div>
        <div class="tir-carte tir-carte--creux tir-carte--serre"><span class="tir-note">Sur la fenêtre</span><span class="tir-val tir-fixe">{euros(h.small.amount)}</span></div>
        <div class="tir-carte tir-carte--creux tir-carte--serre tir-hab-an">
          <span class="tir-note">Sur un an, estimation</span>
          {#if h.small.yearly !== null}<span class="tir-val tir-fixe">≈ {eurosRonds(h.small.yearly)}</span>{:else}<span class="tir-val tir-val--vide">—</span>{/if}
        </div>
      </div>
      <ul class="tir-liste">
        {#each h.small.merchants as m (m.merchant_id)}
          <li>
            <a class="tir-li" href={hrefs.tirelire('operations', undefined, `${lienOperations(h, m.merchant_id)}&max=${h.small.threshold - 1}`)}>
              <span class="tir-col tir-col--colle tir-grandit"><span class="tir-h3 truncate">{m.name ?? 'Commerçant sans nom'}</span><span class="tir-note">{pluriel(m.count, 'achat', 'achats')} · {euros(m.amount)} sur la fenêtre</span></span>
              {#if m.yearly !== null}<span class="tir-col tir-col--colle" style:align-items="flex-end"><span class="tir-fort tir-fixe">≈ {eurosRonds(m.yearly)}</span><span class="tir-note">par an</span></span>{:else}<span class="tir-fort tir-fixe">{euros(m.amount)}</span>{/if}
            </a>
          </li>
        {/each}
      </ul>
      {#if h.small.merchants_total > h.small.merchants.length}<p class="tir-note">Et {pluriel(h.small.merchants_total - h.small.merchants.length, 'autre commerçant', 'autres commerçants')}.</p>{/if}
      <p class="tir-note">{#if h.small.yearly !== null}Estimation : le montant de la fenêtre rapporté à un an, d’après les {h.small.covered_days} jours que tes relevés couvrent. Ce n’est pas une prévision.{:else}Pas d’estimation sur un an : tes relevés ne couvrent que {pluriel(h.small.covered_days, 'jour', 'jours')} de cette fenêtre, il en faut au moins {h.small.min_days}.{/if}</p>
    {:else}
      <p class="tir-note tir-note--texte"><Icon name="ok" size={16} /> Aucun petit achat répété sur cette fenêtre.</p>
    {/if}
  </section>

  <section class="tir-carte tir-hab-jours" aria-label="Par jour de la semaine">
    <div class="tir-ligne tir-ligne--ecart tir-ligne--plis"><h2 class="tir-h2">Par jour de la semaine</h2><span class="tir-note">paiements par carte · {pluriel(h.weekdays.count, 'achat', 'achats')}</span></div>
    {#if h.weekdays.count}
      <div class="tir-histo tir-hab-histo" role="img" aria-label="Paiements par carte par jour de la semaine : {jours.map((t) => `${JOURS[t.rank - 1]} ${euros(t.amount)}`).join(', ')}.">
        {#each jours as t (t.rank)}
          <span class="tir-histo-col" class:tir-histo-col--actuel={marques.includes(t.rank)}>
            <span class="tir-histo-valeur">{eurosRonds(t.amount)}</span>
            <span class="tir-histo-barre" class:tir-histo-barre--actuel={marques.includes(t.rank)} style:height="{(Math.max(0, t.amount) / hautJour) * 100}%"></span>
          </span>
        {/each}
      </div>
      <div class="tir-histo-axe" aria-hidden="true">{#each jours as t (t.rank)}<span class:tir-fort={marques.includes(t.rank)}>{JOURS_COURTS[t.rank - 1]}</span>{/each}</div>
      {#if phraseJours(h.weekdays)}<p class="tir-note tir-note--texte">{phraseJours(h.weekdays)}</p>{/if}
      <p class="tir-note">Seuls les paiements par carte comptent ici, à leur date d’achat : c’est toi qui choisis ce jour-là. Un prélèvement ou un virement tombe à la date fixée par quelqu’un d’autre.</p>
      <details class="tir-donnees">
        <summary>Tableau des données</summary>
        <table class="tir-tab tir-tab--simple">
          <caption class="tir-sr">Paiements par carte par jour de la semaine</caption>
          <thead><tr><th scope="col">Jour</th><th scope="col">Montant</th><th scope="col">Achats</th><th scope="col">Moy./jour</th></tr></thead>
          <tbody>{#each jours as t (t.rank)}<tr><th scope="row">{JOURS[t.rank - 1]}</th><td>{euros(t.amount)}</td><td>{t.count}</td><td>{t.per_day === null ? '—' : euros(t.per_day)}</td></tr>{/each}</tbody>
        </table>
      </details>
    {:else}
      <p class="tir-note tir-note--texte">Aucun paiement par carte sur cette fenêtre. Seuls eux comptent ici : leur date d’achat est un choix, pas celle d’un prélèvement.</p>
    {/if}
  </section>

  <section class="tir-carte tir-hab-semaines" aria-label="Par semaine du mois">
    <div class="tir-ligne tir-ligne--ecart tir-ligne--plis"><h2 class="tir-h2">Par semaine du mois</h2><span class="tir-note">dépenses variables · {euros(h.weeks.amount)}</span></div>
    {#if h.weeks.count}
      <ul class="tir-col" style:gap="10px">
        {#each semaines as t (t.rank)}
          <li class="tir-hab-semaine">
            <span class="tir-note tir-fixe" class:tir-fort={t.rank === forte}>{nomSemaine(t)}</span>
            <span class="tir-hb" aria-hidden="true"><span class="tir-hb-plein" class:tir-hb-plein--fort={t.rank === forte} style:width="{(Math.max(0, t.amount) / hauteSemaine) * 100}%"></span></span>
            <span class="tir-fort tir-fixe">{eurosRonds(t.amount)}</span>
          </li>
        {/each}
      </ul>
      {#if phraseSemaines(h.weeks)}<p class="tir-note tir-note--texte">{phraseSemaines(h.weeks)}</p>{/if}
      <p class="tir-note">Selon le jour du mois (date d’achat pour les cartes). La dernière tranche ne compte que 0 à 3 jours par mois : compare la moyenne par jour.</p>
      <details class="tir-donnees">
        <summary>Tableau des données</summary>
        <table class="tir-tab tir-tab--simple">
          <caption class="tir-sr">Dépenses variables par semaine du mois</caption>
          <thead><tr><th scope="col">Jours</th><th scope="col">Montant</th><th scope="col">Achats</th><th scope="col">Moy./jour</th></tr></thead>
          <tbody>{#each h.weeks.buckets as t (t.rank)}<tr><th scope="row">{nomSemaineLong(t)}</th><td>{euros(t.amount)}</td><td>{t.count}</td><td>{t.per_day === null ? '—' : euros(t.per_day)}</td></tr>{/each}</tbody>
        </table>
      </details>
    {:else}
      <p class="tir-note tir-note--texte">Aucune dépense variable sur cette fenêtre.</p>
    {/if}
  </section>

  <section class="tir-carte tir-hab-rares" aria-label="Opérations inhabituelles">
    <div class="tir-ligne tir-ligne--ecart"><h2 class="tir-h2">Opérations inhabituelles</h2>{#if h.unusual.total}<span class="tir-etq tir-etq--attente">{h.unusual.total}</span>{/if}</div>
    {#if h.unusual.total}
      <ul class="tir-liste">
        {#each h.unusual.operations as o (o.id)}
          <li>
            <a class="tir-li tir-li--haut" href={hrefs.tirelire('operations', o.id)}>
              <span class="tir-pas tir-pas--attente"><Icon name="alerte" size={18} /></span>
              <span class="tir-col tir-col--colle tir-grandit"><span class="tir-h3">{o.merchant_name ?? 'Opération sans commerçant'}</span><span class="tir-note">{dateCourte(o.date)} · {raisonDe(o.reason)}</span></span>
              <span class="tir-fort tir-fixe">{euros(-o.amount)}</span>
            </a>
          </li>
        {/each}
      </ul>
      {#if h.unusual.total > h.unusual.operations.length}<p class="tir-note">Les {h.unusual.operations.length} plus récentes sur {h.unusual.total}.</p>{/if}
    {:else}
      <p class="tir-note tir-note--texte"><Icon name="ok" size={16} /> Rien d’inhabituel sur cette fenêtre.</p>
    {/if}
    <p class="tir-note">Un achat est signalé quand il dépasse nettement ton habitude chez ce commerçant (au moins 4 autres achats, 20 € et 1,5 fois le panier médian), ou à défaut dans sa catégorie (8 autres achats, 50 €). Les charges fixes ne sont pas jugées ici.</p>
  </section>
</div>
