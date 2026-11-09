#!/usr/bin/env node
/**
 * Keep new modules small. Existing large modules are measured in the baseline
 * and may shrink, but may not grow. Remove entries once files fit the limit.
 */
import { existsSync, readFileSync, readdirSync } from 'node:fs'
import { join, relative } from 'node:path'

const ROOT = process.cwd()
const LIMIT = 300
const BASELINE = join(ROOT, 'scripts', 'file-size-baseline.json')
const ROOTS = ['official', 'integration', 'example', 'support', 'sdk/rust', 'sdk/ui']
const roots = ROOTS.map((path) => join(ROOT, path))
const extensions = new Set(['.rs', '.ts', '.tsx'])
const ignored = new Set([
  '.git',
  'node_modules',
  'dist',
  'target',
  'gen',
  'test-results',
  'artifacts',
  'coverage',
])

function walk(directory, files = []) {
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    if (ignored.has(entry.name)) continue
    const path = join(directory, entry.name)
    if (entry.isDirectory()) walk(path, files)
    else if (extensions.has(path.slice(path.lastIndexOf('.')))) files.push(path)
  }
  return files
}

const baseline = JSON.parse(readFileSync(BASELINE, 'utf8'))
const failures = []
const files = new Map()
for (const root of roots) {
  if (!existsSync(root)) {
    failures.push(`${relative(ROOT, root)}: required size-check root is missing`)
    continue
  }
  for (const file of walk(root)) {
    const path = relative(ROOT, file).replaceAll('\\', '/')
    const lines = readFileSync(file, 'utf8').split('\n').length
    files.set(path, lines)
  }
}

for (const [path, recorded] of Object.entries(baseline)) {
  const lines = files.get(path)
  if (lines === undefined) {
    failures.push(`${path}: stale baseline entry; remove it from scripts/file-size-baseline.json`)
  } else if (lines <= LIMIT) {
    failures.push(`${path}: now fits the ${LIMIT}-line limit; remove its baseline entry`)
  } else if (!Number.isInteger(recorded) || recorded <= LIMIT) {
    failures.push(`${path}: baseline must be a measured line count greater than ${LIMIT}`)
  }
}

let allowed = 0
for (const [path, lines] of files) {
  if (lines <= LIMIT) continue
  const recorded = baseline[path]
  if (recorded === undefined) {
    failures.push(`${path}: ${lines} lines; split it or add it to scripts/file-size-baseline.json`)
    continue
  }
  allowed += 1
  if (lines > recorded) {
    failures.push(`${path}: grew from ${recorded} to ${lines} lines; baselined files may only shrink`)
  }
}

if (failures.length) {
  console.error('File size check failed:')
  for (const failure of failures) console.error(`- ${failure}`)
  process.exit(1)
}
console.log(`File size check passed: ${allowed} baselined legacy files, limit ${LIMIT} lines.`)
