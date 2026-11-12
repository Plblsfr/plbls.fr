<script module lang="ts">
  const v3 = {
    hash: '<path d="M5 9h14M5 15h14M10 4 8 20M16 4l-2 16"/>',
    vol: '<path d="M11 5 6 9H3v6h3l5 4V5zM15.5 8.5a5 5 0 0 1 0 7M18.5 5.5a9 9 0 0 1 0 13"/>',
    mic: '<path d="M12 3a3 3 0 0 0-3 3v6a3 3 0 0 0 6 0V6a3 3 0 0 0-3-3zM5 11a7 7 0 0 0 14 0M12 18v3"/>',
    micoff: '<path d="M3 3l18 18M9 9v3a3 3 0 0 0 5.1 2.1M15 9.3V6a3 3 0 0 0-5.7-1.3M5 11a7 7 0 0 0 11.6 5.3M19 11a7 7 0 0 1-.6 2.8M12 18v3"/>',
    casque: '<path d="M4 15v-3a8 8 0 0 1 16 0v3M4 15h3v5H5a1 1 0 0 1-1-1zM20 15h-3v5h2a1 1 0 0 0 1-1z"/>',
    casqueoff: '<path d="M4 15v-3a8 8 0 0 1 13-6.2M20 12v3M4 15h3v5H5a1 1 0 0 1-1-1zM20 15h-3v5h2a1 1 0 0 0 1-1zM3 3l18 18"/>',
    cam: '<path d="M3 7h12v10H3zM15 10l6-3v10l-6-3"/>',
    ecran: '<path d="M3 4h18v12H3zM8 20h8M12 16v4"/>',
    raccroche: '<path d="M3 14c5-5 13-5 18 0l-2 3-4-1v-2a9 9 0 0 0-6 0v2l-4 1z"/>',
    plus: '<path d="M12 5v14M5 12h14"/>',
    cherche: '<path d="M11 4a7 7 0 1 0 0 14 7 7 0 0 0 0-14zM20 20l-4-4"/>',
    cloche: '<path d="M6 16v-5a6 6 0 0 1 12 0v5l2 2H4zM10 21h4"/>',
    reglages: '<path d="M12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6zM12 2v3M12 19v3M4.9 4.9 7 7M17 17l2.1 2.1M2 12h3M19 12h3M4.9 19.1 7 17M17 7l2.1-2.1"/>',
    curseurs: '<path d="M4 6h9M17 6h3M4 12h3M11 12h9M4 18h12M20 18h0M15 4v4M9 10v4M18 16v4"/>',
    gens: '<path d="M9 11a4 4 0 1 0 0-8 4 4 0 0 0 0 8zM2 21v-1a6 6 0 0 1 12 0v1M16 3.5a4 4 0 0 1 0 7.5M22 21v-1a6 6 0 0 0-4-5.6"/>',
    perso: '<path d="M12 12a4 4 0 1 0 0-8 4 4 0 0 0 0 8zM5 21v-1a7 7 0 0 1 14 0v1"/>',
    sourire: '<path d="M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM8.5 14a4.5 4.5 0 0 0 7 0M9 9.5h.01M15 9.5h.01"/>',
    gif: '<path d="M3 5h18v14H3zM10 10H8v4h2v-1.5M13 10v4M16 14v-4h2M16 12h1.5"/>',
    trombone: '<path d="M20 11.5 12 19.5a5 5 0 0 1-7-7l8-8a3.3 3.3 0 0 1 4.7 4.7l-8 8a1.7 1.7 0 0 1-2.4-2.4l7.3-7.3"/>',
    envoi: '<path d="M12 19V5M6 11l6-6 6 6"/>',
    repondre: '<path d="M9 14 4 9l5-5M4 9h10a6 6 0 0 1 6 6v4"/>',
    plus3: '<circle cx="5" cy="12" r="1.7"/><circle cx="12" cy="12" r="1.7"/><circle cx="19" cy="12" r="1.7"/>',
    epingle: '<path d="M9 4h6l-1 5 4 4H6l4-4zM12 13v8"/>',
    crayon: '<path d="M4 20h4L19 9l-4-4L4 16zM13.5 6.5l4 4"/>',
    corbeille: '<path d="M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13"/>',
    drapeau: '<path d="M5 21V4M5 4h11l-2 4 2 4H5"/>',
    x: '<path d="M6 6l12 12M18 6 6 18"/>',
    ok: '<path d="M5 12.5l4.5 4.5L19 7"/>',
    droite: '<path d="M9 6l6 6-6 6"/>',
    gauche: '<path d="M15 6l-6 6 6 6"/>',
    bas: '<path d="M6 9l6 6 6-6"/>',
    maison: '<path d="M4 11 12 4l8 7v9h-5v-6H9v6H4z"/>',
    bulle: '<path d="M4 5h16v11H9l-5 4z"/>',
    fil: '<path d="M4 4h14v9H9l-5 4zM20 9v9l-3-2h-6"/>',
    film: '<path d="M3 4h18v16H3zM7 4v16M17 4v16M3 9h4M3 15h4M17 9h4M17 15h4"/>',
    lecture: '<path d="M7 4.5v15a1 1 0 0 0 1.5.9l12-7.5a1 1 0 0 0 0-1.8l-12-7.5A1 1 0 0 0 7 4.5z"/>',
    pause: '<rect x="6" y="4" width="4" height="16" rx="1"/><rect x="14" y="4" width="4" height="16" rx="1"/>',
    suivant: '<path d="M5 5l10 7-10 7zM19 5v14"/>',
    precedent: '<path d="M19 5 9 12l10 7zM5 5v14"/>',
    note: '<path d="M9 18V5l11-2v13M9 18a3 3 0 1 1-6 0 3 3 0 0 1 6 0zM20 16a3 3 0 1 1-6 0 3 3 0 0 1 6 0z"/>',
    video: '<path d="M3 5h18v14H3zM10 9l5 3-5 3z"/>',
    pinceau: '<path d="M14 4l6 6-9 9H5v-6zM12 6l6 6"/>',
    tele: '<path d="M3 7h18v12H3zM8 3l4 4 4-4"/>',
    bouclier: '<path d="M12 3l8 3v6c0 5-3.5 8-8 9-4.5-1-8-4-8-9V6z"/>',
    medaille: '<path d="M12 3a6 6 0 1 0 0 12 6 6 0 0 0 0-12zM8.5 14 7 21l5-3 5 3-1.5-7"/>',
    sondage: '<path d="M4 6h10M4 12h16M4 18h7"/>',
    image: '<path d="M3 5h18v14H3zM3 16l5-5 4 4 3-3 6 6M15.5 8a1.5 1.5 0 1 0 0 3 1.5 1.5 0 0 0 0-3z"/>',
    cadenas: '<path d="M6 11h12v10H6zM8.5 11V8a3.5 3.5 0 0 1 7 0v3"/>',
    telecharge: '<path d="M12 4v12M7 11l5 5 5-5M4 20h16"/>',
    envoie: '<path d="M12 20V8M7 13l5-5 5 5M4 4h16"/>',
    actualise: '<path d="M20 11a8 8 0 1 0-2.3 5.7M20 4v7h-7"/>',
    lien: '<path d="M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1"/>',
    lune: '<path d="M20 14.5A8 8 0 0 1 9.5 4a8 8 0 1 0 10.5 10.5z"/>',
    soleil: '<path d="M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8zM12 2v2M12 20v2M2 12h2M20 12h2M5 5l1.4 1.4M17.6 17.6 19 19M5 19l1.4-1.4M17.6 6.4 19 5"/>',
    sortie: '<path d="M15 4h4v16h-4M10 8l-4 4 4 4M6 12h11"/>',
    main: '<path d="M8 13V5.5a1.5 1.5 0 0 1 3 0V11M11 10V4.5a1.5 1.5 0 0 1 3 0V11M14 10.5V6a1.5 1.5 0 0 1 3 0v8a7 7 0 0 1-12 4.5l-2.5-4a1.5 1.5 0 0 1 2.5-1.6L8 15"/>',
    etincelle: '<path d="M12 3l1.8 5.2L19 10l-5.2 1.8L12 17l-1.8-5.2L5 10l5.2-1.8zM19 16l.8 2.2L22 19l-2.2.8L19 22l-.8-2.2L16 19l2.2-.8z"/>',
    onde: '<path d="M4 10v4M8 7v10M12 4v16M16 8v8M20 11v2"/>',
    commande: '<path d="M9 9V6a3 3 0 1 0-3 3h12a3 3 0 1 0-3-3v12a3 3 0 1 0 3-3H6a3 3 0 1 0 3 3z"/>',
    boite: '<path d="M3 13l3-8h12l3 8v6H3zM3 13h5l1 2h6l1-2h5"/>',
    arobase: '<path d="M16 12a4 4 0 1 1-8 0 4 4 0 0 1 8 0zM16 12v1.5a2.5 2.5 0 0 0 5 0V12a9 9 0 1 0-3.5 7.1"/>',
    agrandir: '<path d="M14 4h6v6M10 20H4v-6M20 4l-7 7M4 20l7-7"/>',
    reduire: '<path d="M4 14h6v6M20 10h-6V4M14 10l7-7M3 21l7-7"/>',
    etoile: '<path d="M12 3l2.7 5.6 6.1.9-4.4 4.3 1 6.1L12 17l-5.4 2.9 1-6.1-4.4-4.3 6.1-.9z"/>',
    oeil: '<path d="M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12zM12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6z"/>',
    'file-attente': '<path d="M4 6h12M4 12h12M4 18h8M17 14l5 3-5 3z"/>',
    globe: '<path d="M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM3 12h18M12 3c3 3 3 15 0 18M12 3c-3 3-3 15 0 18"/>',
    mobile: '<path d="M7 3h10v18H7zM11 18h2"/>',
    cle: '<path d="M8 9a4 4 0 1 0 0 6 4 4 0 0 0 0-6zM11.5 12H21M18 12v3M21 12v2"/>',
    bug: '<path d="M9 7a3 3 0 0 1 6 0v1H9zM7 9h10v6a5 5 0 0 1-10 0zM3 12h4M17 12h4M4 7l3 2M20 7l-3 2M4 19l3-2M20 19l-3-2"/>',
    recadre: '<path d="M6 2v16h16M2 6h16v16"/>',
    horloge: '<path d="M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM12 7v5l3 2"/>',
    interdit: '<path d="M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM5.6 5.6l12.8 12.8"/>',
    couronne: '<path d="M3 8l4 4 5-7 5 7 4-4-2 11H5z"/>',
    panneau: '<path d="M3 4h18v16H3zM15 4v16"/>',
    copie: '<path d="M8 8h12v12H8zM4 16V4h12"/>',
    scene: '<path d="M3 4h18v12H3zM7 20h10M9 8l3 3 3-3"/>',
    volmax: '<path d="M11 5 6 9H3v6h3l5 4V5zM16 9a4 4 0 0 1 0 6"/>',
    volcoupe: '<path d="M11 5 6 9H3v6h3l5 4V5zM16 9l5 6M21 9l-5 6"/>',
    pip: '<path d="M3 5h18v14H3zM12 12h7v5h-7z"/>',
    filtre: '<path d="M4 5h16l-6 8v6l-4-2v-4z"/>',
    calque: '<path d="M12 3 3 8l9 5 9-5zM3 13l9 5 9-5M3 17l9 5 9-5"/>',
    gomme: '<path d="M15 4l5 5-10 10H5l-2-2 12-13zM8 11l5 5M10 20h11"/>',
    annule: '<path d="M9 14 4 9l5-5M4 9h11a5 5 0 0 1 0 10h-3"/>',
    cible: '<path d="M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8z"/>',
    trophee: '<path d="M7 4h10v5a5 5 0 0 1-10 0zM7 6H4a3 3 0 0 0 3 4M17 6h3a3 3 0 0 1-3 4M12 14v4M8 21h8M9 18h6"/>',
    info: '<path d="M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM12 11v5M12 8h.01"/>',
    alerte: '<path d="M12 4 2.5 20h19zM12 10v4M12 17h.01"/>',
    replay: '<path d="M4 12a8 8 0 1 0 2.3-5.7M4 4v5h5M10 9l5 3-5 3z"/>',
    menu: '<path d="M4 7h16M4 12h16M4 17h16"/>',
  } as const

  const propres = {
    mail: '<path d="M3 5h18v14H3zM3 7l9 6 9-6"/>',
    minus: '<path d="M5 12h14"/>',
    retablis: '<path d="M15 14l5-5-5-5M20 9H9a5 5 0 0 0 0 10h3"/>',
    ciseaux: '<path d="M6 4a3 3 0 1 0 0 6 3 3 0 0 0 0-6zM6 14a3 3 0 1 0 0 6 3 3 0 0 0 0-6zM8.1 9.1 20 20M20 4 8.1 14.9"/>',
    calendar: '<path d="M3 5h18v16H3zM3 10h18M8 3v4M16 3v4"/>',
    list: '<path d="M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01"/>',
    grid: '<path d="M3 3h7v7H3zM14 3h7v7h-7zM3 14h7v7H3zM14 14h7v7h-7z"/>',
    external: '<path d="M14 4h6v6M20 4l-9 9M18 14v6H4V6h6"/>',
    clap: '<path d="M3 10h18v10H3zM3.5 10 20.5 6.5l-.9-3.7L2.6 6.3zM7.7 5.2l2.2 3.6M13.6 4l2.2 3.6"/>',
    expand: '<path d="M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5"/>',
    file: '<path d="M6 3h8l4 4v14H6zM14 3v4h4"/>',
    down: '<path d="M12 5v14M6 13l6 6 6-6"/>',
    shuffle: '<path d="M3 7h3a5 5 0 0 1 4 2l4 6a5 5 0 0 0 4 2h3M3 17h3a5 5 0 0 0 4-2M14 9a5 5 0 0 1 4-2h3M18 4l3 3-3 3M18 14l3 3-3 3"/>',
    repeat: '<path d="M4 11V9a3 3 0 0 1 3-3h13M17 3l3 3-3 3M20 13v2a3 3 0 0 1-3 3H4M7 21l-3-3 3-3"/>',
    phone: '<path d="M5 4h4l2 5-2.5 1.5a11 11 0 0 0 5 5L15 13l5 2v4a2 2 0 0 1-2 2A16 16 0 0 1 3 6a2 2 0 0 1 2-2z"/>',
    'camera-off': '<path d="M3 7h12v10H3zM15 10l6-3v10l-6-3M3 3l18 18"/>',
  } as const

  const alias = {
    chevron: 'droite',
    more: 'plus3',
    flag: 'drapeau',
    check: 'ok',
    sliders: 'curseurs',
    volume: 'vol',
    search: 'cherche',
    pin: 'epingle',
    users: 'gens',
    reply: 'repondre',
    edit: 'crayon',
    trash: 'corbeille',
    play: 'lecture',
    lock: 'cadenas',
    focus: 'agrandir',
    minimize: 'reduire',
    'mic-off': 'micoff',
    headphones: 'casque',
    'headphones-off': 'casqueoff',
    camera: 'cam',
    monitor: 'ecran',
    eye: 'oeil',
    clip: 'trombone',
    smile: 'sourire',
    up: 'envoi',
    'phone-off': 'raccroche',
    chart: 'sondage',
    next: 'suivant',
    restart: 'precedent',
  } as const satisfies Record<string, keyof typeof v3>

  export type IconName = keyof typeof v3 | keyof typeof propres | keyof typeof alias

  const traces: Record<string, string> = { ...v3, ...propres }
  const pleins = new Set<string>(['plus3', 'lecture', 'pause'])
  const epais = new Set<string>(['envoi', 'ok'])

  export function cleIcone(name: IconName): string {
    return name in alias ? alias[name as keyof typeof alias] : name
  }

  export function traceIcone(name: IconName): string {
    return traces[cleIcone(name)] ?? ''
  }
</script>

<script lang="ts">
  let { name, size = 18 }: { name: IconName; size?: number } = $props()
  const cle = $derived(cleIcone(name))
  const plein = $derived(pleins.has(cle))
</script>

<svg class="icon" width={size} height={size} viewBox="0 0 24 24" fill={plein ? 'currentColor' : 'none'} stroke={plein ? 'none' : 'currentColor'} stroke-width={epais.has(cle) ? 2 : 1.8} stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" focusable="false">
  {@html traceIcone(name)}
</svg>
