import { useEffect, useState } from 'react'
import { Archive, LoaderCircle, X } from 'lucide-react'
import { FiexUiClient } from '../../../../sdk/ui/src/index'

interface Context {
  selectionPaths?: string[]
  directoryPath?: string
}
const client = new FiexUiClient()
const formats = [
  { id: 'zip', label: 'ZIP', ext: 'zip' },
  { id: 'rar', label: 'RAR', ext: 'rar' },
  { id: '7z', label: '7z', ext: '7z' },
  { id: 'tar', label: 'TAR', ext: 'tar' },
  { id: 'tar.gz', label: 'TAR.GZ', ext: 'tar.gz' },
  { id: 'tar.bz2', label: 'TAR.BZ2', ext: 'tar.bz2' },
  { id: 'tar.xz', label: 'TAR.XZ', ext: 'tar.xz' },
]
const lastSegment = (value: string) => value.split(/[/\\]/).pop() || value

export function App() {
  const [paths, setPaths] = useState<string[]>([])
  const [format, setFormat] = useState('zip')
  const [level, setLevel] = useState(6)
  const [password, setPassword] = useState('')
  const [full, setFull] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [done, setDone] = useState('')

  useEffect(() => {
    void client
      .invoke<Context>('context.get')
      .then((context) => setPaths(context.selectionPaths || []))
      .catch((cause) => setError(String(cause)))
  }, [])

  const compress = async () => {
    if (!paths.length) return
    setBusy(true)
    setError('')
    setDone('')
    try {
      const selected = formats.find((entry) => entry.id === format) || formats[0]
      const base = paths.length === 1 ? lastSegment(paths[0]).replace(/\.[^.]+$/, '') : 'Archive'
      const destination = await client.invoke<string | null>('dialog.saveFile', {
        suggestedName: `${base}.${selected.ext}`,
        extensions: [selected.ext.split('.').pop() || 'zip'],
      })
      if (!destination) return
      const created = await client.invoke<string>('plugin.invoke', {
        method: 'archive.compress',
        params: {
          paths,
          destination,
          format: selected.id,
          level: password ? null : level,
          password: password || null,
          ignores: full ? [] : [],
        },
      })
      setDone(created)
    } catch (cause) {
      setError(String(cause))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="app">
      <header>
        <div>
          <Archive size={17} />
          <strong>高级压缩</strong>
        </div>
        <button aria-label="关闭" onClick={() => void client.invoke('panel.close')}>
          <X size={16} />
        </button>
      </header>
      {paths.length > 0 ? (
        <>
          <p className="hint">
            将压缩 {paths.length} 个项目：{paths.map(lastSegment).slice(0, 3).join('、')}
            {paths.length > 3 ? '…' : ''}
          </p>
          <label>
            压缩格式
            <select value={format} onChange={(event) => setFormat(event.target.value)}>
              {formats.map((entry) => (
                <option key={entry.id} value={entry.id}>
                  {entry.label}
                </option>
              ))}
            </select>
          </label>
          <label>
            压缩级别
            <input
              type="range"
              min={0}
              max={9}
              value={level}
              disabled={Boolean(password)}
              onChange={(event) => setLevel(Number(event.target.value))}
            />
            <span>{level}</span>
          </label>
          <label>
            ZIP / RAR 密码
            <input
              type="password"
              value={password}
              autoComplete="off"
              placeholder="不设置则不加密"
              onChange={(event) => setPassword(event.target.value)}
            />
          </label>
          <label className="checkbox">
            <input type="checkbox" checked={full} onChange={(event) => setFull(event.target.checked)} />
            包含被忽略的项目
          </label>
          {error && <div className="error">{error}</div>}
          {done && <div className="done">已创建 {done}</div>}
          <footer>
            <button onClick={() => void client.invoke('panel.close')}>取消</button>
            <button className="primary" disabled={busy} onClick={() => void compress()}>
              {busy ? <LoaderCircle size={14} className="spin" /> : <Archive size={14} />}
              开始压缩
            </button>
          </footer>
        </>
      ) : (
        <p className="hint">请选择要压缩的文件或文件夹。</p>
      )}
    </div>
  )
}
