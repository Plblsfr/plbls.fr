import { describe, expect, it } from 'vitest'
import { firstLink, renderMarkdown } from './markdown'

const r = (s: string, p: string[] = []) => renderMarkdown(s, p)

describe('mise en forme', () => {
  it('gère gras, italique, barré, code, citation et listes', () => {
    expect(r('**gras**')).toContain('<strong>gras</strong>')
    expect(r('*it*')).toContain('<em>it</em>')
    expect(r('~~barré~~')).toContain('<del>barré</del>')
    expect(r('`code`')).toContain('<code>code</code>')
    expect(r('```\nbloc\n```')).toContain('<pre><code>bloc')
    expect(r('> citation')).toContain('<blockquote>')
    expect(r('- a\n- b')).toContain('<ul>')
    expect(r('1. a\n2. b')).toContain('<ol>')
  })
  it('conserve les retours à la ligne', () => {
    expect(r('a\nb')).toContain('a<br>b')
  })
  it('ne met pas en forme à l\'intérieur du code', () => {
    const h = r('`**x** ||y||`')
    expect(h).toContain('**x** ||y||')
    expect(h).not.toContain('spoiler')
  })
  it('affiche les spoilers', () => {
    expect(r('||secret||')).toContain('<span class="spoiler">secret</span>')
    expect(r('||un **gras**||')).toContain('<strong>gras</strong>')
    expect(r('a || b')).not.toContain('spoiler')
  })
  it('met en évidence les mentions de rôle, accents et espaces compris', () => {
    expect(r('hé @Modérateur !', ['modérateur'])).toContain('<span class="mention">@Modérateur</span> !')
    expect(r('@Chef de projet merci', ['chef de projet'])).toContain('<span class="mention">@Chef de projet</span> merci')
    expect(r('@Modérateurs', ['modérateur'])).not.toContain('mention')
  })
  it('met en évidence les mentions des pseudos connus seulement', () => {
    expect(r('salut @Alice !', ['alice'])).toContain('<span class="mention">@Alice</span>')
    expect(r('salut @inconnu', ['alice'])).not.toContain('mention')
    expect(r('mail@alice.fr', ['alice'])).not.toContain('mention')
    expect(r('salut @alice.', ['alice'])).toContain('<span class="mention">@alice</span>.')
  })
  it('crée des liens sûrs', () => {
    const h = r('voir https://example.com/x?a=1')
    expect(h).toContain('href="https://example.com/x?a=1"')
    expect(h).toContain('target="_blank"')
    expect(h).toContain('rel="noopener noreferrer nofollow ugc"')
  })
  it('remplace les titres et les images', () => {
    expect(r('# Titre')).not.toContain('<h1')
    expect(r('![x](https://evil.example/p.png)')).not.toContain('<img')
  })
})

describe('sécurité (XSS)', () => {
  const dangerous = [
    '<script>alert(1)</script>',
    '<img src=x onerror=alert(1)>',
    '<svg onload=alert(1)>',
    '<iframe src="javascript:alert(1)"></iframe>',
    '<a href="javascript:alert(1)">x</a>',
    '[x](javascript:alert(1))',
    '[x](data:text/html;base64,PHNjcmlwdD5hbGVydCgxKTwvc2NyaXB0Pg==)',
    '[x](vbscript:alert(1))',
    '<div style="x" onclick="alert(1)">x</div>',
    '<span class="spoiler" onmouseover="alert(1)">x</span>',
    '<input type="text" autofocus onfocus="alert(1)">',
    '<style>*{display:none}</style>',
    '<math><mi//xlink:href="data:x,<script>alert(1)</script>">',
    '**<img src=x onerror=alert(1)>**',
    '||<img src=x onerror=alert(1)>||',
    '> <script>alert(1)</script>',
    '`</code><script>alert(1)</script>`',
    '[a](https://x.example "t" onmouseover="alert(1)")',
  ]
  for (const payload of dangerous) {
    it(`neutralise ${payload.slice(0, 40)}`, () => {
      const div = document.createElement('div')
      div.innerHTML = r(payload)
      expect(div.querySelector('script,img,svg,iframe,style,math,form,object,embed')).toBeNull()
      for (const el of div.querySelectorAll('*')) {
        for (const a of el.getAttributeNames()) {
          expect(a.startsWith('on'), `attribut ${a}`).toBe(false)
          expect(a).not.toBe('style')
        }
        if (el.tagName === 'A') expect(el.getAttribute('href') ?? 'https://ok').toMatch(/^(https?:|mailto:)/)
      }
      expect(div.querySelector('input')).toBeNull()
    })
  }
  it('affiche le HTML brut comme du texte', () => {
    const div = document.createElement('div')
    div.innerHTML = r('<b>gras ?</b>')
    expect(div.querySelector('b')).toBeNull()
    expect(div.textContent).toContain('<b>gras ?</b>')
  })
  it('n\'autorise aucune classe venant de l\'utilisateur', () => {
    const div = document.createElement('div')
    div.innerHTML = r('<span class="plbls-modal__fond">x</span>')
    expect(div.querySelector('.plbls-modal__fond')).toBeNull()
  })
})

describe('firstLink', () => {
  it('trouve le premier lien hors code', () => {
    expect(firstLink('regarde https://a.example/x, puis https://b.example')).toBe('https://a.example/x')
    expect(firstLink('`https://code.example` https://vrai.example')).toBe('https://vrai.example')
    expect(firstLink('```\nhttps://bloc.example\n```')).toBeNull()
    expect(firstLink('rien')).toBeNull()
    expect(firstLink('ftp://x.example')).toBeNull()
  })
})
