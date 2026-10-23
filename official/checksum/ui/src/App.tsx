import { useEffect, useState } from 'react'
import { Check, Copy, LoaderCircle } from 'lucide-react'
import { FiexUiClient } from '../../../../sdk/ui/src/index'

interface Context {
  selectionPaths?: string[]
  directoryPath?: string
}
interface Hashes {
  path: string
  size: number
  sha256: string
  sha512: string
  sha1: string
  md5: string
  crc32: string
}

const client = new FiexUiClient()
const algorithms = [
  ['sha256', 'SHA-256'],
  ['sha512', 'SHA-512'],
  ['sha1', 'SHA-1'],
  ['md5', 'MD5'],
  ['crc32', 'CRC32'],
] as const

export function App() {
  const [paths, setPaths] = useState<string[]>([])
  const [rows, setRows] = useState<Hashes[]>([])
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [copied, setCopied] = useState('')

  useEffect(() => {
    void client
      .invoke<Context>('context.get')
      .then(async (context) => {
        const targets = context.selectionPaths?.length
          ? context.selectionPaths
          : context.directoryPath
            ? [context.directoryPath]
            : []
        setPaths(targets)
        if (!targets.length) return
        setBusy(true)
        try {
          const next: Hashes[] = []
          for (const path of targets) {
            next.push(
              await client.invoke<Hashes>('plugin.invoke', {
                method: 'checksum.calculate',
                params: { path },
              }),
            )
          }
          setRows(next)
        } catch (cause) {
          setError(String(cause))
        } finally {
          setBusy(false)
        }
      })
      .catch((cause) => setError(String(cause)))
  }, [])

  const copy = (key: string, text: string) => {
    void client
      .invoke('clipboard.writeText', { text })
      .then(() => {
        setCopied(key)
        window.setTimeout(() => setCopied(''), 1200)
      })
      .catch((cause) => setError(String(cause)))
  }

  return (
    <div className="app">
      {error && <div className="error">{error}</div>}
      {busy && !rows.length && (
        <div className="loading">
          <LoaderCircle size={18} className="spin" />
        </div>
      )}
      {!busy && !error && !paths.length && <div className="loading" />}
      {rows.map((row) => (
        <dl className="hashes" key={row.path}>
          {algorithms.map(([key, label]) => {
            const id = `${row.path}:${key}`
            return (
              <div className="hash-row" key={key}>
                <dt>{label}</dt>
                <dd>{row[key]}</dd>
                <button
                  className="copy-one"
                  aria-label={`复制 ${label}`}
                  title={`复制 ${label}`}
                  onClick={() => copy(id, row[key])}
                >
                  {copied === id ? <Check size={13} /> : <Copy size={13} />}
                </button>
              </div>
            )
          })}
        </dl>
      ))}
    </div>
  )
}
