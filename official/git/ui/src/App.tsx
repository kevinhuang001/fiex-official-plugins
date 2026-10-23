import { useCallback, useEffect, useMemo, useState } from 'react'
import {
  Check,
  ChevronDown,
  GitBranch,
  GitCommitHorizontal,
  LoaderCircle,
  Minus,
  Plus,
  RefreshCw,
  X,
} from 'lucide-react'
import { FiexUiClient } from '../../../../sdk/ui/src/index'

interface StatusEntry {
  path: string
  code: string
}
interface Status {
  root: string
  branch: string
  entries: Record<string, string>
}
interface Refs {
  head: string
  current: string
  branches: string[]
  tags: string[]
}
interface Commit {
  id: string
  shortId: string
  subject: string
  author: string
  timestamp: number
  refs: string
}
interface Identity {
  name: string
  email: string
}
interface Context {
  directoryPath?: string
  selectionPaths?: string[]
  directory?: { nodeId: string }
}

const client = new FiexUiClient()
const changed = (code: string) => {
  if (code === '??') return { letter: 'U', label: '未跟踪', staged: false, unstaged: true }
  const index = code[0] ?? ' '
  const work = code[1] ?? ' '
  const staged = index !== ' ' && index !== '?'
  const unstaged = work !== ' ' && work !== '?'
  const names: Record<string, string> = {
    M: '修改',
    A: '新增',
    D: '删除',
    R: '重命名',
    C: '复制',
    T: '类型变更',
    U: '冲突',
  }
  const letter = unstaged ? work : index
  if (letter === ' ') return null
  const name = names[letter] ?? letter
  return {
    letter,
    label: staged && unstaged ? `已暂存并${name}` : staged ? `已暂存${name}` : `未暂存${name}`,
    staged,
    unstaged,
  }
}

export function App() {
  const [context, setContext] = useState<Context>({})
  const [status, setStatus] = useState<Status | null>(null)
  const [refs, setRefs] = useState<Refs | null>(null)
  const [history, setHistory] = useState<Commit[]>([])
  const [identity, setIdentity] = useState<Identity>({ name: '', email: '' })
  const [message, setMessage] = useState('')
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)
  const [loaded, setLoaded] = useState(false)
  const [historyOpen, setHistoryOpen] = useState(false)

  const git = useCallback(
    <T,>(method: string, params: Record<string, unknown> = {}) =>
      client.invoke<T>('plugin.invoke', { method, params }),
    [],
  )
  const root = context.directoryPath || ''

  const load = useCallback(async () => {
    if (!root) return
    setBusy(true)
    setError('')
    try {
      const next = await git<Status | null>('git.status', { root })
      setStatus(next)
      if (next) {
        const [nextRefs, nextHistory, nextIdentity] = await Promise.all([
          git<Refs>('git.refs', { root }),
          git<Commit[]>('git.history', { root, skip: 0 }),
          git<Identity>('git.identity', { root }),
        ])
        setRefs(nextRefs)
        setHistory(nextHistory)
        setIdentity(nextIdentity)
      }
    } catch (cause) {
      setError(String(cause))
    } finally {
      setBusy(false)
      setLoaded(true)
    }
  }, [git, root])

  useEffect(() => {
    void client
      .invoke<Context>('context.get')
      .then((next) => setContext(next))
      .catch((cause) => setError(String(cause)))
  }, [])
  useEffect(() => {
    if (root) void load()
  }, [root, load])

  const changes = useMemo(
    () =>
      status
        ? Object.entries(status.entries)
            .sort(([a], [b]) => a.localeCompare(b))
            .flatMap(([path, code]) => {
              const state = changed(code)
              return state ? [{ path, code, ...state }] : []
            })
        : [],
    [status],
  )
  const staged = changes.filter((item) => item.staged)
  const unstaged = changes.filter((item) => item.unstaged)
  const run = async (action: () => Promise<unknown>) => {
    setBusy(true)
    setError('')
    try {
      await action()
      await load()
    } catch (cause) {
      setError(String(cause))
    } finally {
      setBusy(false)
    }
  }
  const toggle = (items: StatusEntry[], staged: boolean) => {
    const paths = items.map((item) => item.path)
    if (!paths.length) return
    void run(() =>
      git(staged ? 'git.unstage' : 'git.stage', { root, paths, all: false }),
    )
  }
  const commit = () => {
    if (!message.trim()) return
    void run(async () => {
      await git('git.commit', {
        root,
        message,
        name: identity.name,
        email: identity.email,
      })
      setMessage('')
    })
  }
  const init = () => void run(() => git('git.init', { root }))
  const openRoot = () => {
    void client.invoke('navigation.open', { path: root }).then(() => client.invoke('panel.close'))
  }
  const openCommit = (commit: Commit) => {
    void git<string>('git.browse', { root, reference: commit.id })
      .then((path) => client.invoke('navigation.open', { path }))
      .then(() => client.invoke('panel.close'))
      .catch((cause) => setError(String(cause)))
  }

  return (
    <div className="app">
      <header>
        <div className="brand">
          <GitBranch size={17} />
          <strong>源代码管理</strong>
          {refs && <span className="branch">{refs.current}</span>}
        </div>
        <button aria-label="刷新" onClick={() => void load()} disabled={busy}>
          <RefreshCw size={15} className={busy ? 'spin' : ''} />
        </button>
        <button aria-label="关闭" onClick={() => void client.invoke('panel.close')}>
          <X size={16} />
        </button>
      </header>

      {error && <div className="error">{error}</div>}
      {!root && loaded && <p className="hint">当前没有可用的文件夹。</p>}
      {root && loaded && !status && (
        <div className="empty">
          <GitBranch size={34} />
          <h2>此文件夹还不是 Git 仓库</h2>
          <button className="primary" onClick={init} disabled={busy}>
            初始化仓库
          </button>
        </div>
      )}

      {status && (
        <>
          <section>
            <div className="section-title">
              <span>暂存的更改</span>
              <b>{staged.length}</b>
              <button
                aria-label="全部取消暂存"
                title="全部取消暂存"
                disabled={busy || !staged.length}
                onClick={() => toggle(staged, true)}
              >
                <Minus size={14} />
              </button>
            </div>
            <ul>
              {staged.map((item) => (
                <li key={item.path}>
                  <span className="letter">{item.letter}</span>
                  <span className="path" title={item.path}>
                    {item.path.split('/').pop()}
                  </span>
                  <button
                    aria-label={`取消暂存 ${item.path}`}
                    disabled={busy}
                    onClick={() => toggle([item], true)}
                  >
                    <Minus size={13} />
                  </button>
                </li>
              ))}
              {!staged.length && <li className="hint">没有暂存的文件</li>}
            </ul>
          </section>

          <section>
            <div className="section-title">
              <span>更改</span>
              <b>{unstaged.length}</b>
              <button
                aria-label="全部暂存"
                title="全部暂存"
                disabled={busy || !unstaged.length}
                onClick={() => toggle(unstaged, false)}
              >
                <Plus size={14} />
              </button>
            </div>
            <ul>
              {unstaged.map((item) => (
                <li key={item.path}>
                  <span className="letter">{item.letter}</span>
                  <span className="path" title={item.path}>
                    {item.path.split('/').pop()}
                  </span>
                  <button
                    aria-label={`暂存 ${item.path}`}
                    disabled={busy}
                    onClick={() => toggle([item], false)}
                  >
                    <Plus size={13} />
                  </button>
                </li>
              ))}
              {!unstaged.length && <li className="hint">没有未暂存的更改</li>}
            </ul>
          </section>

          <section className="commit">
            <div className="identity">
              <input
                aria-label="提交者姓名"
                value={identity.name}
                placeholder="姓名"
                onChange={(event) => setIdentity({ ...identity, name: event.target.value })}
              />
              <input
                aria-label="提交者邮箱"
                value={identity.email}
                placeholder="邮箱"
                onChange={(event) => setIdentity({ ...identity, email: event.target.value })}
              />
            </div>
            <textarea
              aria-label="提交说明"
              value={message}
              placeholder="提交说明"
              rows={3}
              onChange={(event) => setMessage(event.target.value)}
            />
            <button
              className="primary"
              disabled={busy || !message.trim() || !staged.length}
              onClick={commit}
            >
              {busy ? <LoaderCircle size={14} className="spin" /> : <Check size={14} />}
              提交 {staged.length ? `(${staged.length})` : ''}
            </button>
          </section>

          <section className="history">
            <div className="history-head">
              <button className="section-title toggle" onClick={() => setHistoryOpen(!historyOpen)}>
                <span>
                  <GitCommitHorizontal size={14} /> 历史
                </span>
                <ChevronDown size={14} className={historyOpen ? 'open' : ''} />
              </button>
              <button title="返回工作区" onClick={openRoot}>
                工作区
              </button>
            </div>
            {historyOpen && (
              <ul>
                {history.map((commit) => (
                  <li key={commit.id}>
                    <button className="commit-row" onClick={() => openCommit(commit)}>
                      <span className="hash">{commit.shortId}</span>
                      <span className="path" title={commit.subject}>
                        {commit.subject}
                      </span>
                    </button>
                    <small>{commit.author}</small>
                  </li>
                ))}
              </ul>
            )}
          </section>
        </>
      )}
    </div>
  )
}
