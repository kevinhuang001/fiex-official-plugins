#!/usr/bin/env node
import { existsSync, readFileSync, readdirSync } from 'node:fs'
import { join, relative } from 'node:path'

const root = process.cwd()
const limit = 300
const baselinePath = join(root, 'scripts', 'source-size-baseline.json')
const roots = ['official', 'support', 'example/hello/src']
const skipped = new Set(['.git', 'target', 'node_modules', 'dist', 'gen'])
const files = new Map()
const failures = []

function visit(directory) {
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    if (skipped.has(entry.name)) continue
    const path = join(directory, entry.name)
    if (entry.isDirectory()) visit(path)
    else if (path.endsWith('.rs')) {
      const name = relative(root, path).replaceAll('\\', '/')
      files.set(name, readFileSync(path, 'utf8').split(/\r?\n/).length)
    }
  }
}

for (const name of roots) {
  const path = join(root, name)
  if (!existsSync(path)) failures.push(`${name}: required source tree is missing`)
  else visit(path)
}

const baseline = JSON.parse(readFileSync(baselinePath, 'utf8'))
for (const [name, recorded] of Object.entries(baseline)) {
  const current = files.get(name)
  if (current === undefined) failures.push(`${name}: stale baseline entry; remove it`)
  else if (current <= limit) failures.push(`${name}: now fits ${limit} lines; remove its baseline entry`)
  else if (!Number.isInteger(recorded) || recorded <= limit)
    failures.push(`${name}: baseline must record a measured count greater than ${limit}`)
  else if (current > recorded)
    failures.push(`${name}: grew from ${recorded} to ${current} lines`)
}

let allowed = 0
for (const [name, current] of files) {
  if (current <= limit) continue
  if (!(name in baseline)) failures.push(`${name}: ${current} lines; split it or add a measured baseline`)
  else allowed += 1
}

if (failures.length) {
  console.error('Rust source size check failed:')
  for (const failure of failures) console.error(`- ${failure}`)
  process.exit(1)
}
console.log(`Rust source size check passed: ${allowed} baselined files, ${limit}-line target.`)
