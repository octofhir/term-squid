// Preserve production dependency notices without depending on pnpm's Git-package store index.
import { createRequire } from 'node:module'
import { existsSync, mkdirSync, readFileSync, readdirSync, realpathSync, statSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = fileURLToPath(new URL('..', import.meta.url))
const seen = new Set()
const notices = []
function visit(name, from) {
  const require = createRequire(join(from, 'package.json'))
  const directory = (require.resolve.paths(name) ?? []).map(base => join(base, name)).find(base => existsSync(join(base, 'package.json')))
  if (!directory) return // Optional and peer-only dependencies may be absent.
  const path = realpathSync(directory)
  const pkg = JSON.parse(readFileSync(join(path, 'package.json'), 'utf8'))
  const identity = `${pkg.name}@${pkg.version}`
  if (seen.has(identity)) return
  seen.add(identity)
  const files = readdirSync(path).filter(name => /^(licen[cs]e|copying|notice)(\.|$|-)/i.test(name) && statSync(join(path, name)).isFile())
  const text = files.map(name => readFileSync(join(path, name), 'utf8')).join('\n\n')
  const license = typeof pkg.license === 'string' ? pkg.license : JSON.stringify(pkg.license ?? 'See upstream notices')
  notices.push(`${identity}\nLicense: ${license}\n${text || (pkg.name === '@octofhir/ui-kit' ? 'See Apache-2.0.txt and NOTICE.txt in this directory.' : 'License identifier from the installed package manifest; source notices remain with the package.')}\n`)
  for (const dependency of Object.keys({ ...pkg.dependencies, ...pkg.optionalDependencies, ...pkg.peerDependencies })) visit(dependency, path)
}
const manifest = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8'))
for (const name of Object.keys(manifest.dependencies)) visit(name, root)
const destination = fileURLToPath(new URL('../../crates/backend/static/licenses/THIRD_PARTY.txt', import.meta.url))
mkdirSync(dirname(destination), { recursive: true })
writeFileSync(destination, `Third-party production dependency notices\n\n${notices.sort().join('\n\n------------------------------------------------------------\n\n')}`.trimEnd() + '\n')
console.info(`Preserved notices for ${notices.length} production dependencies`)
