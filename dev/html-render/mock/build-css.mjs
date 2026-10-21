#!/usr/bin/env node
// Usage: node dev/html-render/mock/build-css.mjs — writes mock/sfc.css from every component's style block, unscoped, and prints classes styled by more than one component.
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const here = path.dirname(fileURLToPath(import.meta.url))
const src = path.resolve(here, '../../../src')

const vueFiles = (dir) => fs.readdirSync(dir, { withFileTypes: true }).flatMap((e) => {
  const p = path.join(dir, e.name)
  return e.isDirectory() ? vueFiles(p) : p.endsWith('.vue') ? [p] : []
})

const owners = new Map()
const blocks = vueFiles(src).sort().flatMap((file) => {
  const css = fs.readFileSync(file, 'utf8').match(/<style[^>]*>([\s\S]*?)<\/style>/)?.[1]
  if (!css) return []
  const rel = path.relative(src, file)
  for (const [, cls] of css.replace(/\/\*[\s\S]*?\*\//g, '').matchAll(/(?:^|[},])\s*\.([\w-]+)[^{},]*(?=[{,])/g)) {
    owners.set(cls, new Set([...(owners.get(cls) || []), rel]))
  }
  return [`/* ${rel} */\n${css.replace(/:deep\(([^)]*)\)/g, '$1')}`]
})

fs.writeFileSync(path.join(here, 'sfc.css'), blocks.join('\n'))
const shared = [...owners].filter(([, files]) => files.size > 1)
if (shared.length) console.log(`classes styled by more than one component (last one wins here, scoped in the app): ${shared.map(([c, f]) => `${c} [${[...f].join(', ')}]`).join('; ')}`)
