import { useEffect, useState } from 'react'
import { Hash, LoaderCircle, X } from 'lucide-react'
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
const base = (path: string) => path.split(/[/\\]/).pop() || path
const short = (value: string) => `${value.slice(0, 16)}…${value.slice(-8)}`

export function App() {
  const [paths, setPaths] = useState<string[]>([])
  const [rows, setRows] = useState<Hashes[]>([])
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')

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

  return (
    <div className="app">
      <header>
        <div>
          <Hash size={17} />
          <strong>校验和</strong>
        </div>
        <button aria-label="关闭" onClick={() => void client.invoke('panel.close')}>
          <X size={16} />
        </button>
      </header>
      {error && <div className="error">{error}</div>}
      {busy && (
        <p className="hint">
          <LoaderCircle size={14} className="spin" /> 正在计算…
        </p>
      )}
      {!busy && !error && !paths.length && <p className="hint">请选择一个或多个文件。</p>}
      {rows.map((row) => (
        <section key={row.path}>
          <h2 title={row.path}>{base(row.path)}</h2>
          <small>{row.size.toLocaleString()} 字节</small>
          <dl>
            <dt>SHA-256</dt>
            <dd title={row.sha256}>{short(row.sha256)}</dd>
            <dt>SHA-512</dt>
            <dd title={row.sha512}>{short(row.sha512)}</dd>
            <dt>SHA-1</dt>
            <dd title={row.sha1}>{short(row.sha1)}</dd>
            <dt>MD5</dt>
            <dd title={row.md5}>{short(row.md5)}</dd>
            <dt>CRC32</dt>
            <dd title={row.crc32}>{row.crc32}</dd>
          </dl>
          <button
            className="copy"
            onClick={() => {
              const text = [
                `SHA-256 ${row.sha256}`,
                `SHA-512 ${row.sha512}`,
                `SHA-1 ${row.sha1}`,
                `MD5 ${row.md5}`,
                `CRC32 ${row.crc32}`,
              ].join('\n')
              void navigator.clipboard.writeText(text)
            }}
          >
            复制全部
          </button>
        </section>
      ))}
    </div>
  )
}
