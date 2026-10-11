#!/usr/bin/env python3
"""Check one parent-captured, non-truncated receipt. Does not execute anything."""
import json,sys
from pathlib import Path
assert len(sys.argv)==4,'case exit_code stdout_path'
case,rc,path=sys.argv[1],int(sys.argv[2]),Path(sys.argv[3])
with path.open('rb') as stream: raw=stream.read(1048577)
assert len(raw)<=1048576,'output cap exceeded'
rows=[json.loads(line) for line in raw.splitlines()]
assert rows and all(r['case']==case and r['qualification'] is False for r in rows)
if case=='self':
 setter=next((r for r in rows if r.get('event')=='setter'),None)
 assert setter is not None,'Unknown: setter outcome unavailable'
 assert setter['requested_mib']==16
 if setter['mach_return']!=0:
  assert setter['allocation_budget_bytes']==0 and not setter['old_limit_valid']
  assert not any(r.get('event')=='touch_complete' for r in rows)
  classification='Refused' if setter['mach_return']==8 else 'FailureUnknown'
  assert setter['classification']==classification
  assert rc==(0 if classification=='Refused' else 77)
 else:
  assert setter['classification']=='InstalledOnly' and setter['allocation_budget_bytes']==33554432
  final=next((r for r in rows if r.get('event')=='touch_complete'),None)
  if final:
   assert final['touched_allocation_bytes']==33554432
   if rc==0:assert final['task_info_kr']==0 and final['phys_footprint_valid'] and final['phys_footprint_bytes'] is not None and final['as_unchanged']
  classification='InstalledOnly' if rc==0 else 'TerminationOrFailureObservedOnly'
 print(json.dumps({'case':case,'classification':classification,'qualification':False}))
else:
 assert len(rows)==1 and rc==0
 r=rows[0]
 expected={'normal':'measured_under_threshold','growth':'threshold_crossed','failread':'injected_query_unknown','mismatch':'identity_unknown','deadline':'elapsed_deadline'}
 assert case in expected and r['fixture_pass'] is True and r['reason']==expected[case]
 assert r['reaped'] and r['as_unchanged'] and r['core_zero'] and r['baseline_valid']
 assert r['owned_target_only'] and r['monitor_signal_attempts']==0 and r['pid']>0 and r['start_abstime']>0
 assert 1<=len(r['samples'])<=256
 assert all(a['ns']<=b['ns'] for a,b in zip(r['samples'],r['samples'][1:]))
 assert r['threshold_bytes']==r['baseline_bytes']+8388608
 assert r['allocation_budget_bytes']==({'normal':4194304,'growth':25165824}.get(case,0))
 if case=='normal':
  assert r['lifecycle_signal_attempts']==0 and r['wait_status']==0 and r['touch_ack']
  assert r['baseline_bytes']+1048576<=r['peak_observed_bytes']<r['threshold_bytes']
 else:
  assert r['lifecycle_signal_attempts']==1 and r['wait_status']&127==9
  if case=='growth':assert r['peak_observed_bytes']>=r['threshold_bytes'] and r['touch_ack'] and not r['injected']
  if case in ('failread','mismatch'):assert r['injected'] and r['decision']=='unknown'
 print(json.dumps({'case':case,'fixture_contract_pass':True,'qualification':False}))
