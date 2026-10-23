import { useEffect, useRef, useState } from 'react'
import { Archive, Check, ChevronDown, LoaderCircle } from 'lucide-react'
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

function FormatSelect({
  value,
  onChange,
}: {
  value: string
  onChange: (value: string) => void
}) {
  const [open, setOpen] = useState(false)
  const [up, setUp] = useState(false)
  const root = useRef<HTMLDivElement>(null)
  useEffect(() => {
    if (!open) return
    const close = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node)) setOpen(false)
    }
    document.addEventListener('pointerdown', close)
    return () => document.removeEventListener('pointerdown', close)
  }, [open])
  const current = formats.find((entry) => entry.id === value) || formats[0]
  return (
    <div className="format-select" ref={root}>
      <button
        type="button"
        className="format-trigger"
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label="压缩格式"
        onClick={() => {
          const rect = root.current?.getBoundingClientRect()
          setUp(Boolean(rect && rect.bottom + 200 > window.innerHeight - 8))
          setOpen(!open)
        }}
      >
        {current.label}
        <ChevronDown size={14} className={open ? 'open' : ''} />
      </button>
      {open && (
        <div
          className={`format-menu${up ? ' open-up' : ''}`}
          role="listbox"
          aria-label="压缩格式"
        >
          {formats.map((entry) => (
            <button
              type="button"
              role="option"
              aria-selected={entry.id === value}
              className={`format-option ${entry.id === value ? 'active' : ''}`}
              key={entry.id}
              onClick={() => {
                onChange(entry.id)
                setOpen(false)
              }}
            >
              {entry.label}
              {entry.id === value && <Check size={13} />}
            </button>
          ))}
        </div>
      )}
    </div>
  )
}

export function App() {
  const [paths, setPaths] = useState<string[]>([])
  const [ready, setReady] = useState(false)
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
      .finally(() => setReady(true))
  }, [])

  const compress = async () => {
    if (!paths.length) return
    setBusy(true)
    setError('')
    setDone('')
    const selected = formats.find((entry) => entry.id === format) || formats[0]
    const base = paths.length === 1 ? lastSegment(paths[0]).replace(/\.[^.]+$/, '') : 'Archive'
    let task: { taskId: string } | null = null
    try {
      task = await client.tasks.create(`正在压缩为 ${selected.label}`, {
        detail: `${paths.length} 个项目`,
      })
      const destination = await client.invoke<string | null>('dialog.saveFile', {
        suggestedName: `${base}.${selected.ext}`,
        extensions: [selected.ext.split('.').pop() || 'zip'],
      })
      if (!destination) {
        await client.tasks.finish(task.taskId, { detail: '已取消' })
        return
      }
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
      await client.tasks.finish(task.taskId, { detail: created })
    } catch (cause) {
      setError(String(cause))
      if (task) await client.tasks.finish(task.taskId, { error: String(cause) }).catch(() => {})
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="app">
      {ready &&
        (paths.length > 0 ? (
          <>
            <p className="hint">
              将压缩 {paths.length} 个项目：{paths.map(lastSegment).slice(0, 3).join('、')}
              {paths.length > 3 ? '…' : ''}
            </p>
            <label>
              压缩格式
              <FormatSelect value={format} onChange={setFormat} />
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
              <input
                type="checkbox"
                checked={full}
                onChange={(event) => setFull(event.target.checked)}
              />
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
        ))}
    </div>
  )
}
