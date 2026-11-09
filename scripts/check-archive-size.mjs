#!/usr/bin/env node
import { readdirSync, readFileSync } from 'node:fs'
import { join, relative } from 'node:path'

const root = 'official/archive/src'
const limit = 300
const oversized = []
function visit(directory) {
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name)
    if (entry.isDirectory()) visit(path)
    else if (path.endsWith('.rs')) {
      const lines = readFileSync(path, 'utf8').split('\\n').length
      if (lines > limit) oversized.push([relative('.', path), lines])
    }
  }
}
visit(root)
if (oversized.length) {
  console.error('Archive Rust modules must stay at or below 300 lines:')
  for (const [path, lines] of oversized) console.error(`- ${path}: ${lines} lines`)
  process.exit(1)
}
console.log('Archive Rust module size check passed (300-line limit).')
