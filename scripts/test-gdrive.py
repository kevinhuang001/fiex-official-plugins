#!/usr/bin/env python3
"""Real Drive process + deterministic Core HTTP fixture; no cloud credentials."""
import argparse
import base64
import json
import subprocess
import threading
import importlib.util
from pathlib import Path
spec = importlib.util.spec_from_file_location("ipc", Path(__file__).with_name("test-plugin-process.py"))
ipc = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ipc)
send, receive = ipc.send, ipc.receive
SOURCE_REF = {'mountId':'local','nodeId':'source'}

class Fixture:
    def __init__(self, binary):
        self.child = subprocess.Popen([str(Path(binary).resolve())], stdin=subprocess.PIPE,
                                      stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
        self.watchdog = threading.Timer(120, self.child.kill)
        self.watchdog.start()
        self.sequence = 0
        self.streams = {}
        self.content = b'x' * (262144 + 17)
        self.version = '1'
        self.uploaded = bytearray()
        self.source = getattr(self, 'source', b'')
        self.readers = {}
        self.aria2_streams = {}
        self.aria2_downloads = 0
        self.change_before_commit = False
        self.offline = False
        self.oauth_calls = 0
        self.oauth_denied = False
        # Core authorizes every host call with the request that made it and
        # revokes it when that request returns; only a running background task
        # keeps its authorization. The fixture enforces the same rule so a
        # stream that outlives its `vfs.openRead`/`vfs.beginWrite` request is
        # caught here instead of only in the app.
        self.request = None
        self.tasks = set()
        self.call('plugin.initialize', {'protocolVersion':'6.0'})

    def authorize(self, p):
        authorization = p.get('authorization') or {}
        if 'taskId' in authorization:
            assert authorization['taskId'] in self.tasks, (
                f'host call from an expired task: {authorization}')
            return
        assert authorization.get('operationId') == self.request, (
            f'host call from an expired request: {authorization}')

    def call(self, method, params):
        self.sequence += 1
        ident = self.sequence
        self.request = ident
        try:
            send(self.child.stdin, {'jsonrpc':'2.0','id':ident,'method':method,'params':params})
            while True:
                message = receive(self.child.stdout)
                if 'method' not in message:
                    assert message['id'] == ident, message
                    return message
                try:
                    result = self.host(message['method'], message.get('params', {}))
                    reply = {'result':result}
                except RuntimeError as exc:
                    reply = {'error':{'code':-32000,'message':str(exc)}}
                send(self.child.stdin, {'jsonrpc':'2.0','id':message['id'],**reply})
        finally:
            self.request = None

    def host(self, method, p):
        self.authorize(p)
        if method in {"host.log.report", "host.error.report"}:
            return None
        if method == 'host.oauth.authorize':
            self.oauth_calls += 1
            assert p['authorizationEndpoint'] == 'https://accounts.google.com/o/oauth2/v2/auth'
            assert p['clientId'] == 'fixture'
            assert p['parameters'] == {'access_type':'offline','prompt':'consent'}
            if self.oauth_denied:
                raise RuntimeError('OAuth authorization was denied')
            return {'code':'fixture-code','redirectUri':'http://127.0.0.1:4321/oauth/callback','codeVerifier':'fixture-verifier'}
        if method == 'host.task.start':
            self.tasks.add(p['taskId'])
            return None
        if method == 'host.task.finish':
            self.tasks.discard(p['taskId'])
            return None
        if method == 'fs.openRead':
            assert p['ref'] == SOURCE_REF
            self.readers['source-reader'] = self.source
            return {'handle':'source-reader','size':len(self.source)}
        if method == 'fs.readOpen':
            assert p['handle'] in self.readers
            data = self.readers[p['handle']]
            chunk = data[p['offset']:p['offset']+p['length']]
            return {'base64':base64.b64encode(chunk).decode(),'size':len(chunk),'eof':p['offset']+len(chunk)>=len(data)}
        if method == 'fs.closeRead':
            self.readers.pop(p['handle'], None)
            return None
        if method == 'host.buffer.create':
            raise RuntimeError('fixture uses bounded inline reader transport')
        if method == 'host.net.http.close':
            self.streams.pop(p['stream'], None)
            return None
        if method == 'host.net.http.read':
            body = self.streams[p['stream']]
            part = body[:p['size']]
            self.streams[p['stream']] = body[len(part):]
            return {'base64':base64.b64encode(part).decode()}
        if method == 'host.aria2.open':
            if self.offline:
                raise RuntimeError('network disconnected')
            url = p['url']
            assert 'alt=media' in url or '/export?' in url or '/hello.txt' in url, url
            assert 1 <= p['connections'] <= 16 and 1 <= p['split'] <= 16
            assert p.get('size') is None or p['size'] == len(self.content)
            if 'alt=media' in url:
                assert p['headers'].get('authorization') == 'Bearer fixture-token'
            key = f'aria2-{self.aria2_downloads}'
            self.aria2_downloads += 1
            self.aria2_streams[key] = self.content
            return {'streamId': key, 'size': p.get('size')}
        if method == 'host.aria2.read':
            key = p['streamId']
            data = self.aria2_streams[key]
            chunk = data[:p['size']]
            self.aria2_streams[key] = data[len(chunk):]
            return {'base64': base64.b64encode(chunk).decode(), 'eof': not self.aria2_streams[key]}
        if method == 'host.aria2.close':
            self.aria2_streams.pop(p['streamId'], None)
            return None
        assert method == 'host.net.http.open', method
        if self.offline:
            raise RuntimeError('network disconnected')
        url = p['url']
        status, headers = 200, {}
        body = b'{}'
        if 'oauth2.googleapis.com/token' in url:
            from urllib.parse import parse_qs
            form = parse_qs(base64.b64decode(p['bodyBase64']).decode())
            tokens = {'access_token':'fixture-token','expires_in':3600}
            if form.get('grant_type') == ['authorization_code']:
                assert form['code'] == ['fixture-code']
                assert form['code_verifier'] == ['fixture-verifier']
                assert form['redirect_uri'] == ['http://127.0.0.1:4321/oauth/callback']
                tokens['refresh_token'] = 'browser-refresh'
            body = json.dumps(tokens).encode()
        elif '/about?' in url:
            body = b'{"user":{"displayName":"fixture"}}'
        elif '/files/root?' in url:
            body = json.dumps({'id':'root','name':'My Drive','mimeType':'application/vnd.google-apps.folder','capabilities':{'canListChildren':True,'canAddChildren':True}}).encode()
        elif '/files?q=' in url:
            body = json.dumps({'files':[{'id':'file1','name':'data.bin','mimeType':'application/octet-stream','size':str(len(self.content)),'version':self.version,'capabilities':{'canDownload':True,'canModifyContent':getattr(self,'can_modify',True),'canRename':True,'canDelete':True}}]}).encode()
        elif 'alt=media' in url:
            if 'Range' in p['headers']:
                value = p['headers']['Range'].removeprefix('bytes=')
                start, end = map(int,value.split('-'))
                end = min(end,len(self.content)-1)
                status = 206
                headers['content-range'] = f'bytes {start}-{end}/{len(self.content)}'
                body = self.content[start:end+1]
            else:
                body = self.content
        elif 'uploadType=media' in url:
            assert p['headers']['If-Match'] == f'"{self.version}"'
            assert not base64.b64decode(p['bodyBase64'])
            self.content = b''
            self.version = str(int(self.version)+1)
        elif 'uploadType=resumable' in url:
            self.creating = p['method'] == 'POST'
            if self.creating:
                data = json.loads(base64.b64decode(p['bodyBase64']))
                assert data['name'] == 'new.bin' and data['parents'] == ['root']
                assert 'If-Match' not in p['headers']
            else:
                assert p['headers']['If-Match'] == f'"{self.version}"'
            headers['location'] = 'https://www.googleapis.com/upload/drive/session1'
            self.uploaded = bytearray()
            if self.change_before_commit:
                self.version = str(int(self.version)+1)
        elif '/upload/drive/session1' in url:
            frame = base64.b64decode(p['bodyBase64'])
            self.uploaded.extend(frame)
            value = p['headers']['Content-Range']
            bounds, total = value.split('/')
            total = int(total)
            if len(self.uploaded) == total:
                self.content = bytes(self.uploaded)
                self.version = str(int(self.version)+1)
                body = json.dumps({'id':'file2' if getattr(self,'creating',False) else 'file1','name':'new.bin','mimeType':'application/octet-stream','size':str(len(self.content)),'version':self.version,'capabilities':{'canDownload':True,'canModifyContent':True}}).encode()
            else:
                status = 308
                headers['range'] = f'bytes=0-{len(self.uploaded)-1}'
        else:
            assert '/files/file1?' in url, url
            headers['etag'] = f'"{self.version}"'
            body = json.dumps({'id':'file1','size':str(len(self.content)),'mimeType':'application/octet-stream','version':self.version,'capabilities':{'canEdit':True,'canModifyContent':getattr(self,'can_modify',True),'canDownload':True,'canRename':True,'canDelete':True}}).encode()
        stream = f'http-{len(self.streams)}'
        self.streams[stream] = body
        return {'stream':stream,'status':status,'headers':headers}

    def close(self):
        self.call('plugin.shutdown', {})
        self.child.stdin.close()
        try:
            assert self.child.wait(timeout=10) == 0
        finally:
            self.watchdog.cancel()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary',required=True)
    args = parser.parse_args()
    cfg = {'clientId':'fixture','clientSecret':'secret','refreshToken':'refresh'}
    f = Fixture(args.binary)
    try:
        def call(method, **params):
            return f.call(method, {'connection':cfg,**params})
        validated = call('connection.validate')
        assert 'result' in validated, validated
        # New accounts need only user-owned app credentials; Core receives the
        # generated refresh token privately rather than asking the UI to hold it.
        new_cfg = {'clientId':'fixture','clientSecret':'secret'}
        authorized = f.call('connection.validate', {'connection':new_cfg})
        assert authorized['result']['credentials'] == {'refreshToken':'browser-refresh'}, authorized
        assert f.oauth_calls == 1
        resumed = f.call('connection.validate', {'connection':{**new_cfg, **authorized['result']['credentials']}})
        assert 'result' in resumed and 'credentials' not in resumed['result']
        assert f.oauth_calls == 1
        f.oauth_denied = True
        denied = f.call('connection.validate', {'connection':new_cfg})
        assert 'error' in denied and 'result' not in denied
        f.oauth_denied = False

        f.can_modify = False
        operations = call('vfs.discover',nodeId='file1')['result']['operations']
        assert 'read' in operations and 'write' not in operations
        assert 'error' in call('vfs.beginWrite',nodeId='file1',size=1,ifRevision='1')
        f.can_modify = True
        entries = call('vfs.list',nodeId='',cursor='0')['result']['entries']
        assert entries[0]['nodeId'] == 'file1' and entries[0]['revision'] == '1'
        stream = call('vfs.openRead',nodeId='file1')['result']['streamId']
        parts = []
        for offset in (0,262144):
            result = call('vfs.readStream',streamId=stream,offset=offset,maxBytes=262144)['result']
            parts.append(base64.b64decode(result['base64']))
        assert b''.join(parts) == f.content and result['eof']
        call('vfs.closeRead',streamId=stream)
        f.offline = True
        assert 'error' in call('vfs.stat',nodeId='file1')
        f.offline = False
        def upload(data,if_revision,node='file1'):
            opened=call('vfs.beginWrite',nodeId=node,size=len(data),ifRevision=if_revision)
            if 'error' in opened: return opened
            stream=opened['result']['streamId']
            offset=0
            while offset<len(data):
                chunk=data[offset:offset+262144]
                pushed=call('vfs.writeStream',streamId=stream,offset=offset,
                            base64=base64.b64encode(chunk).decode())
                if 'error' in pushed:
                    call('vfs.abort',streamId=stream)
                    return pushed
                offset+=len(chunk)
            return call('vfs.commit',streamId=stream)
        f.source = b'y' * (262144+17)
        assert 'error' in upload(f.source,'stale')
        assert upload(f.source,'1')['result']['revision'] == '2'
        assert f.content == f.source
        f.change_before_commit = True
        assert 'error' in upload(b'z','2')
        assert f.content == b'y' * (262144+17)
        f.change_before_commit = False
        f.source = b''
        assert upload(f.source,'3')['result']['revision'] == '4'
        assert f.content == b''
        opened = call('vfs.beginWrite',parentId='',name='new.bin',size=3)
        stream = opened['result']['streamId']
        call('vfs.writeStream',streamId=stream,offset=0,base64=base64.b64encode(b'new').decode())
        created = call('vfs.commit',streamId=stream)
        assert created['result']['nodeId'] == 'file2', created
        assert f.content == b'new'
        assert not f.readers
        assert not f.streams
        assert not f.aria2_streams
        assert f.aria2_downloads >= 1, 'binary media must use aria2'
        print('PASS Drive aria2 downloads, offline retry, bounded upload, stale revisions and commit conflicts')
    finally:
        f.close()

if __name__ == '__main__':
    main()
