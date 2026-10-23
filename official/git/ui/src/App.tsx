import { useCallback, useEffect, useMemo, useState } from 'react'
import {
  Check,
  ChevronDown,
  Cloud,
  CloudDownload,
  CloudUpload,
  GitBranch,
  GitCommitHorizontal,
  LoaderCircle,
  Minus,
  Plus,
  RefreshCw,
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
interface RemoteStatus {
  remotes: { name: string; url: string }[]
  branch: string
  upstream: string
  ahead: number
  behind: number
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
  const [remote, setRemote] = useState<RemoteStatus | null>(null)
  const [remoteOpen, setRemoteOpen] = useState(false)
  const [remoteName, setRemoteName] = useState('origin')
  const [remoteUrl, setRemoteUrl] = useState('')

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
        const [nextRefs, nextHistory, nextIdentity, nextRemote] = await Promise.all([
          git<Refs>('git.refs', { root }),
          git<Commit[]>('git.history', { root, skip: 0 }),
          git<Identity>('git.identity', { root }),
          git<RemoteStatus>('git.remoteStatus', { root }),
        ])
        setRefs(nextRefs)
        setHistory(nextHistory)
        setIdentity(nextIdentity)
        setRemote(nextRemote)
        if (nextRemote.remotes.length) {
          setRemoteName((old) =>
            nextRemote.remotes.some((item) => item.name === old)
              ? old
              : nextRemote.remotes[0].name,
          )
          setRemoteUrl(
            (old) =>
              old ||
              nextRemote.remotes.find((item) => item.name === remoteName)?.url ||
              nextRemote.remotes[0].url,
          )
        }
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
  const runTask = async (
    title: string,
    detail: string,
    action: (taskId: string) => Promise<string | void>,
  ) => {
    const task = await client.tasks.create(title, { detail })
    setBusy(true)
    setError('')
    try {
      const result = await action(task.taskId)
      await client.tasks.finish(task.taskId, {
        detail: typeof result === 'string' ? result : undefined,
      })
      await load()
    } catch (cause) {
      await client.tasks.finish(task.taskId, { error: String(cause) })
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
    void runTask('提交更改', message.split('\n')[0], async (taskId) => {
      await client.tasks.update(taskId, { step: '写入提交' })
      const hash = await git<string>('git.commit', {
        root,
        message,
        name: identity.name,
        email: identity.email,
      })
      setMessage('')
      return `已提交 ${hash}`
    })
  }
  const init = () =>
    void runTask('初始化 Git 仓库', root, async (taskId) => {
      await client.tasks.update(taskId, { step: '创建 .git' })
      await git('git.init', { root })
      return '仓库已创建'
    })
  const saveRemote = () => {
    if (!remoteName.trim() || !remoteUrl.trim()) return
    void run(async () => {
      await git('git.remoteSave', {
        root,
        name: remoteName.trim(),
        url: remoteUrl.trim(),
      })
    })
  }
  const sync = (push: boolean) =>
    void runTask(
      push ? '推送分支' : '拉取分支',
      `${remoteName}/${remote?.branch || '当前分支'}`,
      async (taskId) => {
        if (!remoteName.trim()) throw new Error('请先配置远程仓库')
        await client.tasks.update(taskId, { step: '连接远程仓库', progress: 0.25 })
        await git('git.remoteSync', {
          root,
          remote: remoteName,
          push,
          branch: remote?.branch || undefined,
        })
        await client.tasks.update(taskId, { step: '更新本地状态', progress: 0.9 })
        return push ? '推送完成' : '拉取完成'
      },
    )
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
      <div className="repo-line">
        <GitBranch size={15} />
        {refs && <span className="branch">{refs.current}</span>}
        <button aria-label="刷新" onClick={() => void load()} disabled={busy}>
          <RefreshCw size={15} className={busy ? 'spin' : ''} />
        </button>
      </div>

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
          <section className="remote-section">
            <button
              className="section-title toggle"
              onClick={() => setRemoteOpen(!remoteOpen)}
            >
              <span>
                <Cloud size={14} /> 远程
                {remote?.remotes.length ? <b>{remote.remotes.length}</b> : null}
              </span>
              <ChevronDown size={14} className={remoteOpen ? 'open' : ''} />
            </button>
            {remoteOpen && (
              <div className="remote-editor">
                {remote?.remotes.length ? (
                  <select
                    aria-label="远程仓库"
                    value={remoteName}
                    onChange={(event) => {
                      setRemoteName(event.target.value)
                      setRemoteUrl(
                        remote.remotes.find((item) => item.name === event.target.value)?.url || '',
                      )
                    }}
                  >
                    {remote.remotes.map((item) => (
                      <option key={item.name} value={item.name}>
                        {item.name}
                      </option>
                    ))}
                  </select>
                ) : null}
                <input
                  aria-label="远程名称"
                  value={remoteName}
                  placeholder="origin"
                  onChange={(event) => setRemoteName(event.target.value)}
                />
                <input
                  aria-label="远程地址"
                  value={remoteUrl}
                  placeholder="https://github.com/..."
                  onChange={(event) => setRemoteUrl(event.target.value)}
                />
                <button className="primary" disabled={busy} onClick={saveRemote}>
                  保存远程
                </button>
                <div className="remote-actions">
                  <button disabled={busy || !remoteName} onClick={() => sync(false)}>
                    <CloudDownload size={14} /> 拉取
                  </button>
                  <button disabled={busy || !remoteName} onClick={() => sync(true)}>
                    <CloudUpload size={14} /> 推送
                  </button>
                </div>
                {remote?.upstream && (
                  <small>
                    {remote.upstream}
                    {remote.ahead ? ` · 领先 ${remote.ahead}` : ''}
                    {remote.behind ? ` · 落后 ${remote.behind}` : ''}
                  </small>
                )}
              </div>
            )}
          </section>
        </>
      )}
    </div>
  )
}
