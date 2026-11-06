import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { cairnApi, ApiError, setTokenProvider } from '../../lib/api'
import { cairnImages } from './cairn-images'
import { buildTree, cairnError, changedFields, conflictFields, pageTrail, renderDoc, stripTitle, tocOf, type PageItem, type Ticket } from './cairn'
import { cairnSections } from './cairn-nav'
import { cairn } from './cairn.svelte'
import { parseHash, hrefs } from '../../lib/route.svelte'

const json = (status: number, body?: unknown) =>
  new Response(body === undefined ? null : JSON.stringify(body), { status, headers: body === undefined ? {} : { 'Content-Type': 'application/json' } })
let fetchMock: ReturnType<typeof vi.fn>
beforeEach(() => { fetchMock = vi.fn(); vi.stubGlobal('fetch', fetchMock); cairn.actions = []; cairn.spaces = []; cairn.space = '' })
afterEach(() => vi.unstubAllGlobals())

const ticket = (o: Partial<Ticket> = {}): Ticket => ({
  key: 'EST-1', space: 'EST', title: 'T', description: null, status: 'todo', type: 'task', priority: 'normal', assignee: null, labels: [], due: null,
  version: 1, created_by: 'u', created_at: '', updated_at: '', blocked: false, ...o,
})

describe('proxy Cairn', () => {
  it('passe par /cairn avec le cookie et la version dans les PATCH', async () => {
    fetchMock.mockResolvedValue(json(200, ticket()))
    await cairnApi.updateTicket('EST-1', 3, { status: 'done' })
    const [url, init] = fetchMock.mock.calls[0]
    expect(url).toBe('/cairn/tickets/EST-1')
    expect(init.method).toBe('PATCH')
    expect(init.credentials).toBe('include')
    expect(JSON.parse(init.body)).toEqual({ version: 3, status: 'done' })
  })

  it('filtre les tickets et ignore les filtres vides', async () => {
    fetchMock.mockResolvedValue(json(200, []))
    await cairnApi.tickets('EST', { status: 'todo', assignee: 'me', label: '', blocked: 'true' })
    expect(fetchMock.mock.calls[0][0]).toBe('/cairn/spaces/EST/tickets?status=todo&assignee=me&blocked=true')
  })

  it('retire une dépendance avec son sens', async () => {
    fetchMock.mockResolvedValue(json(204))
    await cairnApi.removeDependency('EST-1', 'EST-2', 'blocks')
    expect(fetchMock.mock.calls[0][0]).toBe('/cairn/tickets/EST-1/dependencies/EST-2?direction=blocks')
  })
})

describe('droits', () => {
  it("n'affiche Cairn qu'avec space.read", async () => {
    fetchMock.mockResolvedValueOnce(json(200, { actions: ['ticket.read'] }))
    await cairn.init()
    expect(cairn.visible).toBe(false)
    fetchMock.mockResolvedValueOnce(json(200, { actions: ['space.read', 'page.read'] })).mockResolvedValueOnce(json(200, [{ key: 'EST', name: 'Estompe', archived: false }]))
    await cairn.init()
    expect(cairn.visible).toBe(true)
    expect(cairn.space).toBe('EST')
  })

  it.each([401, 403, 503])("masque Cairn quand les droits répondent %d", async (status) => {
    cairn.actions = ['space.read']
    fetchMock.mockResolvedValue(json(status, { error: 'x' }))
    await cairn.init()
    expect(cairn.visible).toBe(false)
  })

  it('masque les sections non permises', () => {
    const ids = (actions: string[]) => cairnSections((a) => actions.includes(a)).map((s) => s.id)
    expect(ids(['space.read'])).toEqual([])
    expect(ids(['ticket.read'])).toEqual(['mine', 'tickets'])
    expect(ids(['ticket.read', 'page.read'])).toEqual(['mine', 'tickets', 'docs', 'search'])
  })
})

describe('erreurs', () => {
  it('403 et 503 donnent un message clair', () => {
    expect(cairnError(new ApiError(403, 'accès refusé', 'Forbidden'))).toMatch(/Action refusée par Cairn/)
    expect(cairnError(new ApiError(503, 'cairn_unavailable', 'Cairn est injoignable'))).toMatch(/injoignable/)
    expect(cairnError(new ApiError(409, 'cycle : EST-1 → EST-2 → EST-1', 'Conflict'))).toBe('cycle : EST-1 → EST-2 → EST-1')
  })
})

describe('modification concurrente (409)', () => {
  it("n'envoie que les champs modifiés", () => {
    const base = ticket()
    expect(changedFields(base, { title: 'T', status: 'done', description: null, labels: [] })).toEqual({ status: 'done' })
  })

  it('signale les champs modifiés des deux côtés', () => {
    const base = ticket()
    const fresh = ticket({ version: 2, status: 'in_progress', priority: 'high' })
    expect(conflictFields(base, fresh, { status: 'done', title: 'Nouveau' })).toEqual(['status'])
  })
})

describe('pages', () => {
  const page = (id: string, parent_id: string | null): PageItem => ({ id, parent_id, position: 0, title: id, author: '', updated_at: '' })
  it("reconstruit l'arbre avec parent_id", () => {
    const tree = buildTree([page('a', null), page('b', 'a'), page('c', 'b'), page('d', null)])
    expect(tree.map((n) => n.id)).toEqual(['a', 'd'])
    expect(tree[0].children[0].children[0].id).toBe('c')
  })

  it("les images passent par le proxy, les autres sont retirées, le HTML est assaini", () => {
    const html = renderDoc('![a](/api/files/abc-1) ![b](https://evil.example/x.png) <script>alert(1)</script> [l](javascript:alert(1))\n\n# Titre', 'https://chat.plbls.fr')
    expect(html).toContain('src="https://chat.plbls.fr/cairn/files/abc-1"')
    expect(html).not.toContain('evil.example/x.png')
    expect(html).not.toContain('<script')
    expect(html).not.toContain('javascript:')
    expect(html).toContain('<h1')
  })
})

describe('lecture des pages', () => {
  const page = (id: string, parent_id: string | null): PageItem => ({ id, parent_id, position: 0, title: id, author: '', updated_at: '' })
  it("fil d'ariane : la page et ses parents, de la racine à la page", () => {
    const pages = [page('a', null), page('b', 'a'), page('c', 'b'), page('d', null)]
    expect(pageTrail(pages, 'c').map((p) => p.id)).toEqual(['a', 'b', 'c'])
    expect(pageTrail(pages, 'zz')).toEqual([])
  })

  it('retire le premier titre quand il redit celui de la page', () => {
    expect(stripTitle('# Estompe — documentation métier\n\nTexte', 'Documentation métier')).toBe('\nTexte')
    expect(stripTitle('# Autre sujet\n\nTexte', 'Documentation')).toBe('# Autre sujet\n\nTexte')
    expect(stripTitle('Texte\n# Documentation', 'Documentation')).toBe('Texte\n# Documentation')
  })

  it('ancres des titres et sommaire, sans doublon', () => {
    const html = renderDoc('## Un titre\n\ntexte\n\n## Un titre\n\n### Sous-titre', 'https://chat.plbls.fr')
    const toc = tocOf(html)
    expect(toc.map((t) => t.text)).toEqual(['Un titre', 'Un titre', 'Sous-titre'])
    expect(new Set(toc.map((t) => t.id)).size).toBe(3)
    expect(toc[2].level).toBe(3)
  })
})

describe('route', () => {
  it('lit #/cairn/<section>/<id>', () => {
    expect(parseHash('#/cairn/ticket/EST-89')).toEqual({ name: 'cairn', section: 'ticket', id: 'EST-89' })
    expect(parseHash(hrefs.cairn('docs'))).toEqual({ name: 'cairn', section: 'docs', id: undefined })
    expect(parseHash(hrefs.cairn())).toEqual({ name: 'cairn', section: undefined, id: undefined })
  })
})

describe('images en mode Bearer', () => {
  it("télécharge les images du proxy avec l'authentification", async () => {
    setTokenProvider(async () => 'jeton')
    const url = vi.fn(() => 'blob:x')
    vi.stubGlobal('URL', Object.assign(URL, { createObjectURL: url }))
    fetchMock.mockResolvedValue(new Response('img'))
    const div = document.createElement('div')
    div.innerHTML = '<img src="https://chat.plbls.fr/cairn/files/abc-1"><img src="https://autre.example/x.png">'
    cairnImages(div)
    await vi.waitFor(() => expect(div.querySelector('img')!.src).toBe('blob:x'))
    expect(fetchMock).toHaveBeenCalledTimes(1)
    expect(fetchMock.mock.calls[0][0]).toBe('/cairn/files/abc-1')
    expect(fetchMock.mock.calls[0][1].headers.Authorization).toBe('Bearer jeton')
    setTokenProvider(null)
  })
})
