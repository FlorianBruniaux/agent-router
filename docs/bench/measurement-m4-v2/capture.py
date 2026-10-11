"""Read-only, exact-participant host evidence capture for the approved M4 block."""
import argparse,hashlib,json,pathlib,sqlite3,sys,re

DB=(pathlib.Path.home()/'.codex/state_5.sqlite').as_uri()+'?mode=ro'
LOG_ROOT=(pathlib.Path.home()/'.codex/sessions').resolve()
MAX_BYTES=64*1024*1024
FIELDS=('input_tokens','cached_input_tokens','cache_write_input_tokens','output_tokens','reasoning_output_tokens','total_tokens')
def safe_metadata(value,pattern):
    return value if isinstance(value,str) and len(value)<=64 and re.fullmatch(pattern,value) else None
def require(value,reason):
    if not value:raise ValueError(reason)
def usage(value):
    require(isinstance(value,dict) and all(type(value.get(k)) is int and 0<=value[k]<2**63 for k in FIELDS),'incomplete_or_invalid_usage')
    require(value['input_tokens']+value['output_tokens']==value['total_tokens'],'inconsistent_total')
    require(value['cached_input_tokens']<=value['input_tokens'] and value['reasoning_output_tokens']<=value['output_tokens'],'invalid_subset')
    return {k:value[k] for k in FIELDS}
def no_duplicates(pairs):
    result={}
    for k,v in pairs:
        require(k not in result,'duplicate_json_key');result[k]=v
    return result
def parse(raw,identity,parent,agent,model,effort):
    require(0<len(raw)<=MAX_BYTES,'trace_size')
    require(raw.endswith(b'\n'),'pending_partial_record')
    rows=[]
    for line in raw.splitlines():
        require(len(line)<=2*1024*1024,'record_size')
        rows.append(json.loads(line,object_pairs_hook=no_duplicates))
    meta=rows[0].get('payload',{});spawn=meta.get('source',{}).get('subagent',{}).get('thread_spawn',{})
    require(rows[0].get('type')=='session_meta' and meta.get('id')==identity,'trace_identity')
    require(spawn.get('parent_thread_id')==parent and spawn.get('agent_path')==agent,'spawn_binding')
    require(meta.get('history_base') is None,'inherited_usage_origin_unknown')
    sums={k:0 for k in FIELDS};last=None;last_event=None;observed=[];seen={};proof=[];starts=[];ends=[];tool_calls=0
    for row in rows:
        q=row.get('payload',{});kind=row.get('type');ordinal=row.get('ordinal')
        if kind=='response_item' and q.get('type') in ('function_call','custom_tool_call','tool_call'):
            tool_calls+=1
        if kind=='turn_context':
            pair={'model':q.get('model'),'effort':q.get('effort')};observed.append(pair)
            require(pair=={'model':model,'effort':effort},'model_effort_mismatch')
        if kind=='token_usage_record':
            require(type(ordinal) is int and ordinal>=0,'record_ordinal')
            require(q.get('thread_id')==identity,'usage_thread_mismatch')
            u=usage(q.get('usage'));total=usage(q.get('thread_token_usage'));key=q.get('response_id')
            require(isinstance(key,str) and 0<len(key)<256,'response_identity_missing')
            encoded=json.dumps(q,sort_keys=True,separators=(',',':')).encode()
            if key in seen:
                require(seen[key]==encoded,'conflicting_response_snapshot');continue
            seen[key]=encoded
            sums={k:sums[k]+u[k] for k in FIELDS}
            require(sums==total,'response_sum_mismatch')
            if last is not None:require(all(total[k]>=last[k] for k in FIELDS),'counter_decrease')
            last=total
            proof.append({'ordinal':ordinal,'timestamp':safe_metadata(row.get('timestamp'),r'\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?Z'),'response_identity_sha256':hashlib.sha256(key.encode()).hexdigest(),'usage':u,'cumulative':total})
        if kind=='event_msg' and q.get('type')=='token_count' and q.get('info'):
            total=usage(q['info'].get('total_token_usage'));require(last is not None and total==last,'event_counter_mismatch');last_event=total
        if kind=='event_msg' and q.get('type')=='task_started':starts.append(ordinal)
        if kind=='event_msg' and q.get('type')=='task_complete':ends.append(ordinal)
    require(observed,'model_context_missing');require(last is not None,'usage_not_yet_observed')
    require(last_event==last,'pending_counter_flush')
    return {'status':'reliable_host_counters','agent_path':agent,'thread_id':identity,'parent_thread_id':parent,'requested':{'model':model,'effort':effort},'observed':observed,'scope':'exact host participant, not provider invoice','runtime_cli_version':safe_metadata(meta.get('cli_version'),r'\d+\.\d+\.\d+(?:[-+][a-zA-Z0-9.]+)?'),'snapshot_sha256':hashlib.sha256(raw).hexdigest(),'snapshot_bytes':len(raw),'response_record_count':len(proof),'response_records':proof,'cumulative':last,'sum_matches_cumulative':True,'task_started_count':len(starts),'task_complete_count':len(ends),'tool_call_count':tool_calls,'history_origin':'declared_empty' if 'history_base' in meta else 'not_declared','terminal':bool(starts and ends and ends[-1]>starts[-1]),'provider_identity_verified':False}
def capture(parent,agent,model,effort):
    with sqlite3.connect(DB,uri=True,timeout=2) as c:
        rows=c.execute('SELECT t.id,t.rollout_path FROM threads t JOIN thread_spawn_edges e ON e.child_thread_id=t.id WHERE e.parent_thread_id=? AND t.agent_path=?',(parent,agent)).fetchall()
    require(len(rows)==1,'participant_binding_missing_or_ambiguous')
    identity,path=rows[0];file=pathlib.Path(path).resolve();require(file.is_relative_to(LOG_ROOT),'unexpected_trace_location')
    with file.open('rb') as f:raw=f.read(MAX_BYTES+1)
    return parse(raw,identity,parent,agent,model,effort)
def main():
    p=argparse.ArgumentParser();p.add_argument('--parent',required=True);p.add_argument('--agent',required=True);p.add_argument('--model',required=True);p.add_argument('--effort',required=True);p.add_argument('--output',type=pathlib.Path);a=p.parse_args()
    try:r=capture(a.parent,a.agent,a.model,a.effort)
    except (ValueError,OSError,sqlite3.Error,TypeError,AttributeError,KeyError) as e:
        reason=str(e) if isinstance(e,ValueError) and str(e).replace('_','').isalnum() else 'capture_failed'
        r={'status':'not_reliable','reason':reason}
    if a.output:a.output.write_text(json.dumps(r,indent=2)+'\n')
    print(json.dumps({k:v for k,v in r.items() if k!='response_records'},indent=2))
    return 0 if r['status']=='reliable_host_counters' else 3
if __name__=='__main__':sys.exit(main())
