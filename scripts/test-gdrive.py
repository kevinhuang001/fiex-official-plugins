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
        self.change_before_commit = False
        self.offline = False
        self.call('plugin.initialize', {'protocolVersion':'4.0'})

    def call(self, method, params):
        self.sequence += 1
        ident = self.sequence
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

    def host(self, method, p):
        if method == 'host.fs.openRead':
            assert p['ref'] == SOURCE_REF
            self.readers['source-reader'] = self.source
            return {'handle':'source-reader','size':len(self.source)}
        if method == 'host.fs.readOpen':
            assert p['handle'] in self.readers
            data = self.readers[p['handle']]
            chunk = data[p['offset']:p['offset']+p['length']]
            return {'base64':base64.b64encode(chunk).decode(),'size':len(chunk),'eof':p['offset']+len(chunk)>=len(data)}
        if method == 'host.fs.closeRead':
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
        assert method == 'host.net.http.open', method
        if self.offline:
            raise RuntimeError('network disconnected')
        url = p['url']
        status, headers = 200, {}
        body = b'{}'
        if 'oauth2.googleapis.com/token' in url:
            body = json.dumps({'access_token':'fixture-token','expires_in':3600}).encode()
        elif '/about?' in url:
            body = b'{"user":{"displayName":"fixture"}}'
        elif '/files?q=' in url:
            body = json.dumps({'files':[{'id':'file1','name':'data.bin','mimeType':'application/octet-stream','size':str(len(self.content)),'version':self.version}]}).encode()
        elif 'alt=media' in url:
            value = p['headers']['Range'].removeprefix('bytes=')
            start, end = map(int,value.split('-'))
            end = min(end,len(self.content)-1)
            status = 206
            headers['content-range'] = f'bytes {start}-{end}/{len(self.content)}'
            body = self.content[start:end+1]
        elif 'uploadType=media' in url:
            assert p['headers']['If-Match'] == f'"{self.version}"'
            assert not base64.b64decode(p['bodyBase64'])
            self.content = b''
            self.version = str(int(self.version)+1)
        elif 'uploadType=resumable' in url:
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
            else:
                status = 308
                headers['range'] = f'bytes=0-{len(self.uploaded)-1}'
        else:
            assert '/files/file1?' in url, url
            headers['etag'] = f'"{self.version}"'
            body = json.dumps({'id':'file1','size':str(len(self.content)),'mimeType':'application/octet-stream','version':self.version,'capabilities':{'canEdit':True}}).encode()
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
        assert call('connection.validate')['result']['valid']
        entries = call('fs.list',directoryId='',cursor='0')['result']['entries']
        assert entries[0]['nodeId'] == 'file1' and entries[0]['revision'] == '1'
        stream = call('fs.openRead',nodeId='file1')['result']['streamId']
        parts = []
        for offset in (0,262144):
            result = call('fs.readStream',streamId=stream,offset=offset,maxBytes=262144)['result']
            parts.append(base64.b64decode(result['base64']))
        assert b''.join(parts) == f.content and result['eof']
        call('fs.closeRead',streamId=stream)
        f.offline = True
        assert 'error' in call('fs.stat',nodeId='file1')
        f.offline = False
        f.source = b'y' * (262144+17)
        assert 'error' in call('fs.writeFile',nodeId='file1',source=SOURCE_REF,size=len(f.source),ifRevision='stale')
        assert 'error' in call('fs.writeFile',nodeId='file1',source=SOURCE_REF,size=1,ifRevision='1')
        assert call('fs.writeFile',nodeId='file1',source=SOURCE_REF,size=len(f.source),ifRevision='1')['result']['revision'] == '2'
        assert f.content == f.source
        f.change_before_commit = True
        f.source = b'z'
        assert 'error' in call('fs.writeFile',nodeId='file1',source=SOURCE_REF,size=1,ifRevision='2')
        assert f.content == b'y' * (262144+17)
        f.change_before_commit = False
        f.source = b''
        assert call('fs.writeFile',nodeId='file1',source=SOURCE_REF,size=0,ifRevision='3')['result']['revision'] == '4'
        assert f.content == b''
        assert not f.readers
        assert not f.streams
        print('PASS Drive ranges, offline retry, bounded upload, stale revisions and commit conflicts')
    finally:
        f.close()

if __name__ == '__main__':
    main()
