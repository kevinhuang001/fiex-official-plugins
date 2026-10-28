import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { FiexUiClient } from '../../../../sdk/ui/src/index'
import '@xterm/xterm/css/xterm.css'
import './style.css'

const client = new FiexUiClient()
const root = document.getElementById('root')!
const status = document.createElement('div')
status.role = 'status'
status.textContent = 'Connecting…'
const host = document.createElement('div')
host.className = 'terminal'
root.append(status, host)
const terminal = new Terminal({cursorBlink:true, fontSize:14, fontFamily:'Menlo, Consolas, monospace', scrollback:5000})
const fit = new FitAddon()
terminal.loadAddon(fit)
terminal.open(host)
let sessionId = ''
let stopped = false
let pending: number[] = []
const encoder = new TextEncoder()
const input = terminal.onData((text) => {
  const bytes = Array.from(encoder.encode(text))
  if (pending.length + bytes.length <= 65536) pending.push(...bytes)
})
const resize = new ResizeObserver(() => { if (host.clientWidth && host.clientHeight) fit.fit() })
resize.observe(host)
const invoke = <T>(method:string, params:Record<string,unknown>) => client.invoke<T>('plugin.invoke',{method,params})
function close() {
  if (stopped) return
  stopped = true
  resize.disconnect()
  input.dispose()
  if (sessionId) void invoke('ssh.close',{sessionId}).catch(() => {})
  terminal.dispose()
}
window.addEventListener('pagehide', close, {once:true})
async function run() {
  const context = await client.invoke<{state?:{sessionId?:string}}>('context.get')
  sessionId = context.state?.sessionId || ''
  if (!sessionId) throw new Error('Open SSH from Connections')
  if (stopped) { void invoke('ssh.close',{sessionId}); return }
  fit.fit()
  await invoke('ssh.start',{sessionId,cols:terminal.cols,rows:terminal.rows})
  terminal.focus()
  while (!stopped) {
    // Only one exchange in flight; input and output remain ordered.
    const data = pending
    pending = []
    const value = await invoke<{dataBase64:string;ready:boolean;ended:boolean;error?:string}>('ssh.exchange',{
      sessionId, dataBase64:btoa(String.fromCharCode(...data)), cols:terminal.cols, rows:terminal.rows,
    })
    if (stopped) return
    if (value.dataBase64) terminal.write(Uint8Array.from(atob(value.dataBase64),char=>char.charCodeAt(0)))
    status.textContent = value.error || (value.ended ? 'Disconnected' : value.ready ? '' : 'Connecting…')
    if (value.ended) { await invoke('ssh.close',{sessionId}); break }
    await new Promise(resolve=>setTimeout(resolve,24))
  }
}
void run().catch(error=>{
  status.textContent = String(error)
  if (sessionId) void invoke('ssh.close',{sessionId}).catch(()=>{})
})
