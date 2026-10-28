#!/usr/bin/env python3
"""Real WebDAV/OpenList processes with a deterministic Core/HTTP broker."""
import argparse
import base64
import json
import importlib.util
from pathlib import Path
spec=importlib.util.spec_from_file_location('drive_fixture',Path(__file__).with_name('test-gdrive.py'))
module=importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

CHUNK=256*1024

class Connection(module.Fixture):
    def __init__(self,binary,kind):
        self.kind=kind
        self.files={}
        self.pending={}
        self.source=b'edited while offline\n'
        super().__init__(binary)
    def host(self,method,p):
        self.authorize(p)
        if method=='host.net.http.open' and p.get('bodyStream'):
            if self.offline: raise RuntimeError('network disconnected')
            handle=f'body-{len(self.pending)}'
            self.pending[handle]={
                'method':p['method'],'url':p['url'],'headers':p.get('headers',{}),
                'expected':p['contentLength'],'body':bytearray(),'response':None}
            return {'handle':handle}
        if method=='host.net.http.body':
            pending=self.pending[p['handle']]
            pending['body'].extend(base64.b64decode(p.get('base64','')))
            if p.get('eof'):
                assert len(pending['body'])==pending['expected'], 'body length must match contentLength'
                pending['response']=self.finish_stream(pending)
            return None
        if method=='host.net.http.response':
            pending=self.pending.pop(p['handle'])
            assert pending['response'] is not None, 'body must be finished first'
            return pending['response']
        if method=='host.net.http.close':
            key=p.get('handle') or p.get('stream')
            self.pending.pop(key,None)
            self.streams.pop(key,None)
            return None
        if method=='host.net.http.open':
            if self.offline: raise RuntimeError('network disconnected')
            return self.inline_request(p)
        if method=='host.storage.get':
            raise AssertionError('credentials must come from the connection, not storage')
        if method=='host.secret.get':
            raise AssertionError('credentials must come from the connection, not the keychain')
        return super().host(method,p)
    def inline_request(self,p):
        url=p['url']
        status,headers,body=200,{},b'{}'
        if p['method']=='HEAD':
            headers={'etag':f'"{self.version}"','content-length':str(len(self.content))}
        elif p['method']=='PROPFIND':
            body=(f'<?xml version="1.0"?><d:multistatus xmlns:d="DAV:"><d:response>'
                  f'<d:href>/hello.txt</d:href><d:propstat><d:prop>'
                  f'<d:getcontentlength>{len(self.content)}</d:getcontentlength>'
                  f'<d:current-user-privilege-set><d:privilege><d:all/></d:privilege></d:current-user-privilege-set>'
                  f'<d:getetag>"{self.version}"</d:getetag>'
                  f'<d:getlastmodified>Mon, 01 Jan 2024 00:00:00 GMT</d:getlastmodified>'
                  f'<d:resourcetype/></d:prop><d:status>HTTP/1.1 200 OK</d:status>'
                  f'</d:propstat></d:response></d:multistatus>').encode()
        elif p['method']=='GET':
            if url.endswith('/api/me'):
                body=json.dumps({'code':200,'data':{'role':2,'permission':4294967295}}).encode()
            else:
                headers={'content-length':str(len(self.content))}
                body=self.content
        elif p['method']=='MKCOL' or p['method']=='MOVE' or p['method']=='DELETE':
            status=201 if p['method']=='MKCOL' else 204
        elif url.endswith('/api/auth/login'):
            body=b'{"code":200,"data":{"token":"fixture"}}'
        elif url.endswith('/api/me'):
            body=json.dumps({'code':200,'data':{'role':2,'permission':4294967295}}).encode()
        elif url.endswith('/api/fs/get'):
            body=json.dumps({'code':200,'data':{'modified':self.version,'size':len(self.content),
                'is_dir':False,'raw_url':'https://fixture.invalid/hello.txt'}}).encode()
        elif url.endswith('/api/fs/list'):
            body=json.dumps({'code':200,'data':{'write':True,
                'content':[{'name':'hello.txt','size':len(self.content),'is_dir':False,'modified':'2024-01-01T00:00:00Z'}]}}).encode()
        elif p['method']=='PUT':
            raise AssertionError('uploads must stream through bodyStream')
        else:
            raise AssertionError(url)
        stream=f'http-{len(self.streams)}'
        self.streams[stream]=body
        return {'stream':stream,'status':status,'headers':headers}
    def finish_stream(self,pending):
        if self.offline: raise RuntimeError('network disconnected')
        headers=dict(pending['headers'])
        body=bytes(pending['body'])
        if pending['method']=='PUT':
            if self.kind=='webdav':
                assert headers.get('If-Match')==f'"{self.version}"', headers
            else:
                assert headers.get('File-Path')=='%2Fhello.txt', headers
            self.content=body
            self.version=str(int(self.version)+1)
            payload=b'{"code":200}'
        else:
            raise AssertionError(f'unexpected streamed {pending["method"]} {pending["url"]}')
        stream=f'http-{len(self.streams)}'
        self.streams[stream]=payload
        return {'stream':stream,'status':200,'headers':{}}

def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('--binary',required=True)
    parser.add_argument('--kind',choices=['webdav','openlist'],required=True)
    args=parser.parse_args()
    f=Connection(args.binary,args.kind)
    config={'url':'https://fixture.invalid','username':'user','password':'password','timeout':30}
    try:
        def call(method,**p):
            return f.call(method,{'connection':config,'nodeId':'hello.txt',**p})
        def upload(data,if_revision):
            opened=call('vfs.beginWrite',size=len(data),ifRevision=if_revision)
            if 'error' in opened: return opened
            stream=opened['result']['streamId']
            offset=0
            while offset<len(data):
                chunk=data[offset:offset+CHUNK]
                pushed=call('vfs.writeStream',streamId=stream,offset=offset,
                            base64=base64.b64encode(chunk).decode())
                if 'error' in pushed:
                    call('vfs.abort',streamId=stream)
                    return pushed
                offset+=len(chunk)
            return call('vfs.commit',streamId=stream)
        before=call('vfs.stat')['result']['revision']
        # Opening a file streams it through openRead/readStream: the plugin's
        # HTTP body outlives the request that opened it.
        stream=call('vfs.openRead')['result']['streamId']
        parts=[]
        offset=0
        while True:
            result=call('vfs.readStream',streamId=stream,offset=offset,maxBytes=CHUNK)['result']
            chunk=base64.b64decode(result['base64'])
            parts.append(chunk)
            offset+=len(chunk)
            if result['eof']: break
        assert b''.join(parts)==f.content and offset==len(f.content)
        call('vfs.closeRead',streamId=stream)
        assert not f.streams, f.streams
        assert 'error' in upload(f.source,'stale')
        f.offline=True
        assert 'error' in upload(f.source,before)
        f.offline=False
        uploaded=upload(f.source,before)
        assert 'result' in uploaded, uploaded
        after=uploaded['result']['revision']
        assert after!=before and f.content==f.source
        assert 'error' in upload(f.source,before)
        assert not f.files and not f.streams and not f.pending
        print('PASS',args.kind,'offline retry, version checks and streamed upload')
    finally:f.close()
if __name__=='__main__':main()
