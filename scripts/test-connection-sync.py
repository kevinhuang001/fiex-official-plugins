#!/usr/bin/env python3
"""Real WebDAV/OpenList processes with a deterministic Core/HTTP broker."""
import argparse
import importlib.util
import json
from pathlib import Path
spec=importlib.util.spec_from_file_location('drive_fixture',Path(__file__).with_name('test-gdrive.py'))
module=importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

class Connection(module.Fixture):
    def __init__(self,binary,kind):
        self.kind=kind
        self.files={}
        self.source=b'edited while offline\n'
        super().__init__(binary)
    def host(self,method,p):
        if method=='host.storage.get':
            return {'username':'user','timeout':30}.get(p['key'],'')
        if method=='host.secret.get':
            return 'password'
        if method=='host.fs.openRead':
            assert p['path']=='source.dat'
            self.files['selected-snapshot']=self.source
            return {'handle':'selected-snapshot','size':len(self.source)}
        if method=='host.fs.closeRead':
            del self.files[p['handle']]
            return None
        if method!='host.net.http.open':
            return super().host(method,p)
        if self.offline: raise RuntimeError('network disconnected')
        url=p['url']
        status,headers,body=200,{},b'{}'
        if p['method']=='HEAD':
            headers={'etag':f'"{self.version}"','content-length':str(len(self.content))}
        elif url.endswith('/api/auth/login'):
            body=b'{"code":200,"data":{"token":"fixture"}}'
        elif url.endswith('/api/fs/get'):
            body=json.dumps({'code':200,'data':{'modified':self.version,'size':len(self.content)}}).encode()
        elif p['method']=='PUT':
            assert not p.get('bodyBase64'), 'file must not be buffered into inline JSON'
            assert p['bodyReadHandle'] in self.files
            if self.kind=='webdav': assert p['headers']['If-Match']==f'"{self.version}"'
            else: assert p['headers']['File-Path']=='%2Fhello.txt'
            self.content=self.files[p['bodyReadHandle']]
            self.version=str(int(self.version)+1)
            body=b'{"code":200}'
        else: raise AssertionError(url)
        stream=f'http-{len(self.streams)}'
        self.streams[stream]=body
        return {'stream':stream,'status':status,'headers':headers}

def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('--binary',required=True)
    parser.add_argument('--kind',choices=['webdav','openlist'],required=True)
    args=parser.parse_args()
    f=Connection(args.binary,args.kind)
    try:
        def call(method,**p):
            return f.call(method,{'connection':{'url':'https://fixture.invalid'},'nodeId':'hello.txt',**p})
        before=call('fs.stat')['result']['revision']
        assert 'error' in call('fs.writeFile',source='source.dat',ifRevision='stale')
        f.offline=True
        assert 'error' in call('fs.writeFile',source='source.dat',ifRevision=before)
        f.offline=False
        after=call('fs.writeFile',source='source.dat',ifRevision=before)['result']['revision']
        assert after!=before and f.content==f.source
        assert 'error' in call('fs.writeFile',source='source.dat',ifRevision=before)
        assert not f.files and not f.streams
        print('PASS',args.kind,'offline retry, version checks and handle-streamed upload')
    finally:f.close()
if __name__=='__main__':main()
