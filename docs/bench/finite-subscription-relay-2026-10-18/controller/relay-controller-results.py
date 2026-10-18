"""Offline-only benchmark results owner. No dispatch, credentials or Runtime authority.

Only caller-created private temporary test directories are accepted in this preparation.
One results.json is authoritative for observations; results.lock contains no state.
"""
import copy
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import secrets
import stat
import tempfile

ROOT=Path(__file__).resolve().parent
MANIFEST_SHA='ad14ea5af4f50d677cab07a2f3541d2c010a18245f67117e60835662067d3542'
TEMPLATE_SHA='f1cab8db41dd19b8e58274604200484d58f3c77bf979f47f8222e43f7ee0d255'
CONTROLLER_SHA='c2943ce88c95b7e9aeefaf5ea827b7e9b2b0780737a84b8fe9c2bd5bbd945828'
GATEWAY_SHA='97f2c8bdc55f067848f12cffb5a41086aaf5adec3ed8cb36567f6540b2975f5e'
MAX_FILE=4*1024*1024
META='relay_results'


def canonical(value):
    return json.dumps(value,sort_keys=True,separators=(',',':'),ensure_ascii=False,allow_nan=False).encode()


def sha(value):return hashlib.sha256(value).hexdigest()


def hexhash(value):return isinstance(value,str) and re.fullmatch('[0-9a-f]{64}',value) is not None


def integer(value,maximum=10**18):return type(value) is int and 0<=value<=maximum


def controller():
    source=(ROOT/'relay-controller-offline.py').read_bytes()
    if sha(source)!=CONTROLLER_SHA or sha((ROOT/'subscription_gateway.py').read_bytes())!=GATEWAY_SHA:
        raise ValueError('controller_or_gateway_preimage_changed')
    # Execute the exact checked observer bytes, not a second read after verification.
    namespace={'__name__':'pinned_offline_observer','__file__':str(ROOT/'relay-controller-offline.py')}
    exec(compile(source,namespace['__file__'],'exec'),namespace)
    return namespace


class ResultsManager:
    def __init__(self,directory,manifest_bytes,template_bytes,*,budget_microcredits,
                 configuration,verified_configuration_sha256):
        self.dirfd=None;self.lockfd=None;self.closed=False;self.poisoned=False;self.last_digest=None
        try:
            self.common=controller()
            if not integer(budget_microcredits) or budget_microcredits<=0:
                raise ValueError('explicit_positive_budget_required')
            if (not isinstance(manifest_bytes,bytes) or not isinstance(template_bytes,bytes)
                    or sha(manifest_bytes)!=MANIFEST_SHA or sha(template_bytes)!=TEMPLATE_SHA):
                raise ValueError('sealed_input_mismatch')
            self.manifest=self.common['strict_json'](manifest_bytes)
            self.template=self.common['strict_json'](template_bytes)
            self.common['budget_check'](self.manifest['assignments'])
            if (len(self.template['rows'])!=24 or
                    any(any(canonical(row[k])!=canonical(v) for k,v in assignment.items())
                        for row,assignment in zip(self.template['rows'],self.manifest['assignments']))):
                raise ValueError('sealed_assignment_mismatch')
            keys={'profile','manifest_sha256','template_sha256','controller_sha256','gateway_sha256',
                  'account_sha256','observed_campaign_budget_microcredits','observed_tokens_per_trial','wall_seconds_per_trial'}
            if (not isinstance(configuration,dict) or set(configuration)!=keys
                    or configuration['profile']!='codex_subscription_relay'
                    or configuration['manifest_sha256']!=MANIFEST_SHA or configuration['template_sha256']!=TEMPLATE_SHA
                    or configuration['controller_sha256']!=CONTROLLER_SHA or configuration['gateway_sha256']!=GATEWAY_SHA
                    or not hexhash(configuration['account_sha256'])
                    or type(configuration['observed_campaign_budget_microcredits']) is not int
                    or configuration['observed_campaign_budget_microcredits']!=budget_microcredits
                    or type(configuration['observed_tokens_per_trial']) is not int or configuration['observed_tokens_per_trial']!=6000
                    or type(configuration['wall_seconds_per_trial']) is not int or configuration['wall_seconds_per_trial']!=60
                    or not hexhash(verified_configuration_sha256)
                    or sha(canonical(configuration))!=verified_configuration_sha256):
                raise ValueError('reviewed_configuration_mismatch')
            self.config=copy.deepcopy(configuration);self.budget=budget_microcredits
            self.config_sha=verified_configuration_sha256
            directory=Path(directory)
            if (directory.is_symlink() or directory.resolve().parent!=Path(tempfile.gettempdir()).resolve()
                    or not directory.name.startswith('relay-controller-results-test-')):
                raise ValueError('temporary_test_directory_only')
            self.dirfd=os.open(directory,os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW)
            info=os.fstat(self.dirfd)
            if info.st_uid!=os.getuid() or stat.S_IMODE(info.st_mode)!=0o700:
                raise ValueError('private_directory_required')
            self.lockfd=os.open('results.lock',os.O_CREAT|os.O_RDWR|os.O_NOFOLLOW,0o600,dir_fd=self.dirfd)
            self.file_metadata(self.lockfd)
            fcntl.flock(self.lockfd,fcntl.LOCK_EX|fcntl.LOCK_NB)
            if any(name.startswith('.results-') for name in os.listdir(self.dirfd)):
                raise ValueError('incomplete_atomic_write_requires_review')
            existing=self.read_raw()
            if existing is None:
                self.state=copy.deepcopy(self.template)
                self.state[META]={'schema_version':1,'scope':'offline_test_only','configuration':self.config,
                                  'configuration_sha256':self.config_sha,'stop_reason':None,'checksum':''}
                for row in self.state['rows']:
                    row[META]={'run_id':None,'approval_digest':None,'receipts':[],'pending_request':None,
                               'usage_unknown':False,'cleanup_confirmation':None,'native_status':None}
                self.save()
            else:
                self.state=self.common['strict_json'](existing)
                self.last_digest=sha(existing);self.validate()
                active=[r for r in self.state['rows'] if r['status']=='in_progress']
                if active:
                    for row in active:row['status']='stopped';row[META]['usage_unknown']=True
                    self.state[META]['stop_reason']='reopened_in_progress'
                    self.save()
        except Exception:
            self.close();raise

    def __enter__(self):return self
    def __exit__(self,*args):self.close()

    def close(self):
        if self.lockfd is not None:os.close(self.lockfd);self.lockfd=None
        if self.dirfd is not None:os.close(self.dirfd);self.dirfd=None
        self.closed=True

    def guard(self):
        if self.closed or self.poisoned:raise ValueError('closed_or_uncommitted_results')

    @staticmethod
    def file_metadata(fd):
        info=os.fstat(fd)
        if (not stat.S_ISREG(info.st_mode) or info.st_uid!=os.getuid() or info.st_nlink!=1
                or stat.S_IMODE(info.st_mode)!=0o600 or info.st_size>MAX_FILE):
            raise ValueError('private_single_link_file_required')

    def read_raw(self):
        try:fd=os.open('results.json',os.O_RDONLY|os.O_NOFOLLOW,dir_fd=self.dirfd)
        except FileNotFoundError:return None
        try:
            self.file_metadata(fd)
            with os.fdopen(fd,'rb',closefd=False) as stream:data=stream.read(MAX_FILE+1)
            if len(data)>MAX_FILE:raise ValueError('results_bound')
            return data
        finally:os.close(fd)

    def checksum(self):
        value=copy.deepcopy(self.state);value[META]['checksum']=''
        return sha(canonical(value))

    def save(self):
        self.guard()
        try:
            current=self.read_raw()
            if (sha(current) if current is not None else None)!=self.last_digest:
                raise ValueError('results_changed_outside_lock')
            self.state[META]['checksum']=self.checksum();self.validate()
            raw=canonical(self.state)+b'\n'
            if len(raw)>MAX_FILE:raise ValueError('results_bound')
            name='.results-'+secrets.token_hex(12)
            fd=os.open(name,os.O_CREAT|os.O_EXCL|os.O_WRONLY|os.O_NOFOLLOW,0o600,dir_fd=self.dirfd)
            try:
                with os.fdopen(fd,'wb',closefd=False) as stream:
                    stream.write(raw);stream.flush();os.fsync(fd)
            finally:os.close(fd)
            os.replace(name,'results.json',src_dir_fd=self.dirfd,dst_dir_fd=self.dirfd)
            os.fsync(self.dirfd)
            self.last_digest=sha(raw)
        except Exception:
            self.poisoned=True;raise

    def fold(self,row):
        records=row[META]['receipts']
        if not isinstance(records,list) or len(records)>64:raise ValueError('receipt_bound')
        totals=dict.fromkeys(('input_tokens','cached_input_tokens','output_tokens','total_tokens'),0)
        nano=0;ids=set()
        for index,receipt in enumerate(records,1):
            required={'assignment_id','request_id','request_body_sha256','response_id','model','effort',
                      'observed_response_model','sse_sha256','sse_bytes','usage','cumulative_usage',
                      'observed_equivalent_microcredits','unit','authority'}
            if (not isinstance(receipt,dict) or set(receipt)!=required
                    or receipt['assignment_id']!=row['assignment_id'] or receipt['model']!=row['model']
                    or receipt['effort']!=row['effort'] or type(receipt['request_id']) is not int
                    or receipt['request_id']!=index or not hexhash(receipt['request_body_sha256'])
                    or not hexhash(receipt['sse_sha256']) or not integer(receipt['sse_bytes'],8*1024*1024)
                    or receipt['sse_bytes']==0 or receipt['unit']!=self.common['UNIT'] or receipt['authority'] is not False):
                raise ValueError('receipt_binding')
            rid=receipt['response_id'];actual=receipt['observed_response_model']
            if (not isinstance(rid,str) or not 1<=len(rid)<=256 or rid in ids
                    or not isinstance(actual,str) or not(actual==row['model'] or actual.startswith(row['model']+'-'))):
                raise ValueError('response_binding')
            usage=receipt['usage']
            if (not isinstance(usage,dict) or set(usage)!=set(totals)|{'reasoning_output_tokens'}
                    or any(not integer(usage[k],1_000_000_000) for k in totals)):
                raise ValueError('receipt_usage')
            inp,cached,out,total=(usage[k] for k in ('input_tokens','cached_input_tokens','output_tokens','total_tokens'))
            reason=usage['reasoning_output_tokens']
            if (total==0 or inp+out!=total or cached>inp
                    or reason is not None and (not integer(reason) or reason>out)):
                raise ValueError('receipt_usage')
            for k in totals:totals[k]+=usage[k]
            if canonical(receipt['cumulative_usage'])!=canonical(totals):raise ValueError('modified_cumulative_usage')
            i,c,o=self.common['RATES'][row['model']]
            nano+=(inp-cached)*i+cached*c+out*o
            if not integer(receipt['observed_equivalent_microcredits']) or receipt['observed_equivalent_microcredits']!=(nano+999)//1000:
                raise ValueError('modified_cumulative_cost')
            ids.add(rid)
        return totals,(nano+999)//1000,ids

    def cleanup_matches(self,row,proof):
        return (isinstance(proof,dict) and set(proof)=={'run_id','approval_digest','proof_sha256','writers_excluded'}
                and proof['run_id']==row[META]['run_id'] and proof['approval_digest']==row[META]['approval_digest']
                and hexhash(proof['proof_sha256']) and proof['writers_excluded'] is True)

    def validate(self):
        if not isinstance(self.state,dict) or set(self.state)!=set(self.template)|{META}:raise ValueError('results_shape')
        for key,value in self.template.items():
            if key!='rows' and canonical(self.state[key])!=canonical(value):raise ValueError('benchmark_top_level_changed')
        meta=self.state[META]
        if (not isinstance(meta,dict) or set(meta)!={'schema_version','scope','configuration','configuration_sha256','stop_reason','checksum'}
                or type(meta['schema_version']) is not int or meta['schema_version']!=1 or meta['scope']!='offline_test_only'
                or canonical(meta['configuration'])!=canonical(self.config) or meta['configuration_sha256']!=self.config_sha
                or meta['checksum']!=self.checksum()):
            raise ValueError('results_integrity_or_configuration')
        if meta['stop_reason'] is not None and (not isinstance(meta['stop_reason'],str) or not re.fullmatch('[a-z0-9_]{1,80}',meta['stop_reason'])):
            raise ValueError('stop_reason')
        rows=self.state['rows']
        if not isinstance(rows,list) or len(rows)!=24:raise ValueError('24_assignments_required')
        seen=set();runs=set();unstarted=False;active=0
        for row,original in zip(rows,self.template['rows']):
            if not isinstance(row,dict) or set(row)!=set(original)|{META}:raise ValueError('row_shape')
            for key,value in original.items():
                if key not in ('status','usage','cost_microcredits') and canonical(row[key])!=canonical(value):
                    raise ValueError('sealed_assignment_changed')
            if row['status'] not in ('not_run','in_progress','completed','failed','stopped'):raise ValueError('row_status')
            rm=row[META]
            if (not isinstance(rm,dict) or set(rm)!={'run_id','approval_digest','receipts','pending_request','usage_unknown','cleanup_confirmation','native_status'}
                    or type(rm['usage_unknown']) is not bool):raise ValueError('row_metadata')
            if row['status']=='not_run':
                unstarted=True
                if rm!={'run_id':None,'approval_digest':None,'receipts':[],'pending_request':None,'usage_unknown':False,'cleanup_confirmation':None,'native_status':None} or row['usage'] is not None or row['cost_microcredits'] is not None:
                    raise ValueError('unstarted_row_changed')
            else:
                if unstarted:raise ValueError('sealed_order_changed')
                if not isinstance(rm['run_id'],str) or not re.fullmatch('[A-Za-z0-9_-]{1,128}',rm['run_id']) or not hexhash(rm['approval_digest']):raise ValueError('run_binding')
                if rm['run_id'] in runs:raise ValueError('run_identity_reused')
                runs.add(rm['run_id'])
                if rm['usage_unknown'] and meta['stop_reason'] is None:raise ValueError('unknown_without_stop')
                active+=row['status']=='in_progress'
                if rm['native_status'] not in (None,'completed','failed','interrupted'):raise ValueError('native_status')
                if rm['cleanup_confirmation'] is not None and not self.cleanup_matches(row,rm['cleanup_confirmation']):raise ValueError('cleanup_binding')
                pending=rm['pending_request']
                if pending is not None and (not isinstance(pending,dict) or set(pending)!={'request_id','request_body_sha256'}
                        or type(pending['request_id']) is not int or pending['request_id']!=len(rm['receipts'])+1
                        or pending['request_id']>64 or not hexhash(pending['request_body_sha256'])):raise ValueError('pending_request')
                if row['status']=='completed' and (pending is not None or not rm['receipts'] or rm['native_status']!='completed' or rm['usage_unknown'] or rm['cleanup_confirmation'] is None):raise ValueError('completion_incomplete')
                if row['status'] in ('failed','stopped') and meta['stop_reason'] is None:raise ValueError('unrecorded_stop')
            totals,cost,ids=self.fold(row)
            if ids & seen:raise ValueError('response_id_reused_across_cells')
            seen|=ids
            if rm['receipts'] and (canonical(row['usage'])!=canonical(totals) or type(row['cost_microcredits']) is not int or row['cost_microcredits']!=cost):raise ValueError('row_counter_changed')
            if not rm['receipts'] and (row['usage'] is not None or row['cost_microcredits'] is not None):raise ValueError('unobserved_counter')
        if active>1:raise ValueError('concurrent_attempts')

    def row(self,assignment_id):
        self.guard()
        for row in self.state['rows']:
            if row['assignment_id']==assignment_id:return row
        raise ValueError('unknown_assignment')

    def snapshot(self):
        self.guard();return copy.deepcopy(self.state)

    def observation(self):
        self.guard()
        rows=self.state['rows'];known=sum(r['cost_microcredits'] or 0 for r in rows)
        started=[r for r in rows if r['status']!='not_run']
        allows=(self.state[META]['stop_reason'] is None and known<self.budget
                and any(r['status']=='not_run' for r in rows)
                and all(r['status']=='completed' and r[META]['cleanup_confirmation'] is not None and not r[META]['usage_unknown'] for r in started))
        return {'known_cost_microcredits':known,'usage_unknown':any(r[META]['usage_unknown'] for r in rows),
                'budget_observation_allows_next':allows,'execution_authorized':False,'scope':'offline_test_only',
                'stop_reason':self.state[META]['stop_reason'],'unit':self.common['UNIT'],
                'actual_subscription_debit':None,'runtime_admission':'SEPARATE_OPAQUE_AUTHORITY_REQUIRED'}

    def begin_trial(self,assignment_id,run_id,approval_digest):
        self.guard()
        next_row=next((r for r in self.state['rows'] if r['status']=='not_run'),None)
        if (not self.observation()['budget_observation_allows_next'] or next_row is None
                or next_row['assignment_id']!=assignment_id or not isinstance(run_id,str)
                or not re.fullmatch('[A-Za-z0-9_-]{1,128}',run_id) or not hexhash(approval_digest)
                or any(r[META]['run_id']==run_id for r in self.state['rows'])):
            raise ValueError('attempt_not_allowed')
        next_row['status']='in_progress';next_row[META]['run_id']=run_id;next_row[META]['approval_digest']=approval_digest
        self.save()

    def before_request(self,assignment_id,request_id,request_body_sha256):
        row=self.row(assignment_id);rm=row[META]
        if (self.state[META]['stop_reason'] is not None or row['status']!='in_progress' or rm['pending_request'] is not None
                or type(request_id) is not int or request_id!=len(rm['receipts'])+1 or request_id>64
                or not hexhash(request_body_sha256)):
            raise ValueError('request_not_allowed')
        rm['pending_request']={'request_id':request_id,'request_body_sha256':request_body_sha256};self.save()

    def mark_unknown(self,assignment_id,reason='usage_unknown'):
        row=self.row(assignment_id)
        if row['status']=='not_run':raise ValueError('unstarted_unknown')
        if not isinstance(reason,str) or not re.fullmatch('[a-z0-9_]{1,80}',reason):raise ValueError('reason_code_required')
        row[META]['usage_unknown']=True;row['status']='stopped'
        self.state[META]['stop_reason']=reason;self.save()

    def ingest(self,assignment_id,receipt):
        row=self.row(assignment_id);rm=row[META]
        try:
            if not isinstance(receipt,dict) or len(canonical(receipt))>16384:raise ValueError('receipt_shape')
            for old in rm['receipts']:
                if receipt.get('request_id')==old['request_id'] or receipt.get('response_id')==old['response_id']:
                    if canonical(receipt)==canonical(old):return self.observation()
                    raise ValueError('conflicting_duplicate')
            pending=rm['pending_request']
            if (row['status']!='in_progress' or self.state[META]['stop_reason'] is not None or pending is None
                    or receipt.get('request_id')!=pending['request_id'] or receipt.get('request_body_sha256')!=pending['request_body_sha256']):
                raise ValueError('request_not_recorded')
            candidate=copy.deepcopy(row);candidate[META]['receipts'].append(copy.deepcopy(receipt))
            totals,cost,ids=self.fold(candidate)
            if any(ids & {r['response_id'] for r in other[META]['receipts']} for other in self.state['rows'] if other is not row):
                raise ValueError('response_reused')
        except (ValueError,KeyError,TypeError,RecursionError):
            self.mark_unknown(assignment_id,'receipt_unknown_or_conflicting');raise ValueError('receipt_unknown_or_conflicting') from None
        rm['receipts']=candidate[META]['receipts'];rm['pending_request']=None
        row['usage']=totals;row['cost_microcredits']=cost
        if totals['total_tokens']>=6000:self.state[META]['stop_reason']='observed_token_limit'
        if sum(r['cost_microcredits'] or 0 for r in self.state['rows'])>=self.budget:self.state[META]['stop_reason']='observed_campaign_budget'
        if len(rm['receipts'])>=64:self.state[META]['stop_reason']='response_count_limit'
        self.save();return self.observation()

    def finish_trial(self,assignment_id,native_status,cleanup_confirmation):
        row=self.row(assignment_id);rm=row[META]
        if row['status'] not in ('in_progress','stopped') or native_status not in ('completed','failed','interrupted'):
            raise ValueError('terminal_not_allowed')
        rm['native_status']=native_status
        if self.cleanup_matches(row,cleanup_confirmation):rm['cleanup_confirmation']=copy.deepcopy(cleanup_confirmation)
        if rm['pending_request'] is not None or not rm['receipts']:rm['usage_unknown']=True
        if rm['usage_unknown']:row['status']='stopped';self.state[META]['stop_reason']='usage_unknown'
        elif native_status!='completed':row['status']='failed';self.state[META]['stop_reason']='native_non_success'
        elif rm['cleanup_confirmation'] is None:row['status']='stopped';self.state[META]['stop_reason']='cleanup_unconfirmed'
        else:row['status']='completed'
        self.save();return self.observation()
