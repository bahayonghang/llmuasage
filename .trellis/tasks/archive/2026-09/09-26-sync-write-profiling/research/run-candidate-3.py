import datetime, hashlib, json, os, pathlib, subprocess, sys
root=pathlib.Path.cwd()
r=root/'.trellis/tasks/09-26-sync-write-profiling/research'
identity=json.loads((r/'candidate-3-build-identity.json').read_text(encoding='utf-8'))
exe=root/identity['executable']['path']
os.environ['PATH']='C:/Users/lyh/.cargo/bin'+os.pathsep+os.environ['PATH']
entries={'writer':'store::sync_writer::profiling::tests::sync_writer_replay_ab_acceptance','host':'store::sync_writer::profiling::tests::sync_writer_host_skew_ab_acceptance','parser':'parsers::writer_benchmark::writer_parser_ab_acceptance'}
key=sys.argv[1]
test=entries[key]
name='candidate-3-'+key
assert not (r/(name+'.log')).exists(), 'Do not overwrite existing run'
def now(): return datetime.datetime.now(datetime.timezone.utc).isoformat()
def sha(b): return hashlib.sha256(b).hexdigest()
def git(*args): return subprocess.check_output(['git',*args],cwd=root,stderr=subprocess.DEVNULL)
def verify():
    raw=[f['path'] for f in identity['files'] if sha((root/f['path']).read_bytes())!=f['sha256']]
    lf=[f['path'] for f in identity['files'] if sha((root/f['path']).read_bytes().replace(b'\r\n',b'\n'))!=f['normalized_lf_sha256']]
    tracked=git('ls-files','-z','--',*identity['scope']).decode().split(chr(0))
    untracked=git('ls-files','--others','--exclude-standard','-z','--',*identity['scope']).decode().split(chr(0))
    paths=sorted(set(p for p in tracked+untracked if p and (root/p).is_file()))
    patch=git('diff','--binary','--no-ext-diff','HEAD','--',*identity['scope'])
    check={'at_utc':now(),'raw_mismatches':raw,'lf_mismatches':lf,'file_set_match':paths==[f['path'] for f in identity['files']],'patch_match':sha(patch)==identity['tracked_source_diff_sha256'],'executable_match':sha(exe.read_bytes())==identity['executable']['sha256'],'head_match':git('rev-parse','HEAD').decode().strip()==identity['git_head']}
    check['passed']=not raw and not lf and all(check[k] for k in ['file_set_match','patch_match','executable_match','head_match'])
    return check
before=verify()
assert before['passed'],before
list_cmd=[str(exe),test,'--list','--exact']
lst=subprocess.run(list_cmd,capture_output=True,text=True)
(r/(name+'-list.log')).write_text(lst.stdout+lst.stderr+'\nEXIT_CODE='+str(lst.returncode)+'\n',encoding='utf-8')
assert lst.returncode==0 and '1 test, 0 benchmarks' in lst.stdout
cmd=[str(exe),test,'--exact','--ignored','--test-threads=1','--nocapture']
record={'name':name,'command':cmd,'list_command':list_cmd,'list_exit_code':lst.returncode,'started_at_utc':now(),'log':name+'.log','executable_sha256':identity['executable']['sha256'],'identity_before':before}
(r/(name+'-run-start.json')).write_text(json.dumps(record,indent=2)+'\n',encoding='utf-8')
print(json.dumps(record),flush=True)
with (r/(name+'.log')).open('wb') as log:
    result=subprocess.run(cmd,stdout=log,stderr=subprocess.STDOUT)
    log.write(('\nEXIT_CODE='+str(result.returncode)+'\n').encode())
record.update({'finished_at_utc':now(),'exit_code':result.returncode,'identity_after':verify()})
with (r/'candidate-3-runs.jsonl').open('a',encoding='utf-8') as ledger: ledger.write(json.dumps(record)+'\n')
print(json.dumps(record),flush=True)
assert record['identity_after']['passed'],record['identity_after']
sys.exit(result.returncode)
