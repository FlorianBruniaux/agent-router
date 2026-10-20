"""Offline private-pipe observer. No process launch, auth, network or ledger writer.

Its reports are observations only. Runtime accounting and cleanup authority must
be connected by the owning Rust integration before any next trial can be admitted.
"""
import base64
import binascii
import copy
import hashlib
import json
import math
import os
import re
import select
import stat
import sys
import time

MAX_LINE = 300000
MAX_CHUNK = 65536
MAX_STREAM = 8 * 1024 * 1024
# Nanocredits per token from the pinned relay, Standard rate card 2026-10-15.
RATES = {'gpt-6-luna': (2500, 250, 12500), 'gpt-6-astra': (250000, 25000, 1250000)}
UNIT = 'credit_equivalents_not_subscription_debit_or_usd'


def strict_json(raw):
    def unique(pairs):
        value = {}
        for key, item in pairs:
            if key in value: raise ValueError('duplicate_key')
            value[key] = item
        return value
    def constant(_): raise ValueError('invalid_json_constant')
    return json.loads(raw, object_pairs_hook=unique, parse_constant=constant)


def integer(value, maximum=1_000_000_000):
    return type(value) is int and 0 <= value <= maximum


class PipeObserver:
    def __init__(self, assignment_id, model, effort, *, started_at, clock=time.monotonic, on_chunk=None):
        if (not isinstance(assignment_id,str) or not re.fullmatch(r't\d{2}-[LA][mx]-r1-H[01]',assignment_id)
                or model not in RATES or effort not in ('medium','xhigh')
                or type(started_at) not in (int,float) or not math.isfinite(started_at)):
            raise ValueError('binding')
        arm=assignment_id.split('-')[1]
        if (model != ('gpt-6-luna' if arm[0]=='L' else 'gpt-6-astra')
                or effort != ('medium' if arm[1]=='m' else 'xhigh')):
            raise ValueError('binding')
        self.assignment_id,self.model,self.effort=assignment_id,model,effort
        self.clock,self.deadline,self.on_chunk=clock,started_at+60,on_chunk
        self.pending=b'';self.active=False;self.closed=False;self.stopped=False
        self.unknown=False;self.reason=None;self.next_id=1;self.terminals=0
        self.totals=dict.fromkeys(('input_tokens','cached_input_tokens','output_tokens','total_tokens'),0)
        self.nano_credits=0;self.response_ids=set();self.stream_bytes=0
        self.request_hash=None;self.stream_hash=hashlib.sha256();self.last_receipt=None

    def fail(self, reason, *, unknown=True):
        self.stopped=True;self.unknown=self.unknown or unknown;self.reason=reason
        raise ValueError(reason)

    def poll(self):
        now=self.clock()
        if type(now) not in (int,float) or not math.isfinite(now) or now>=self.deadline:
            self.fail('trial_deadline',unknown=self.active or bool(self.pending))
        if self.stopped:self.fail(self.reason or 'stopped',unknown=False)

    def begin(self, request_id, request_body_sha256):
        self.poll()
        if (self.closed or self.active or type(request_id) is not int or request_id != self.next_id
                or request_id>64 or not isinstance(request_body_sha256,str)
                or not re.fullmatch('[0-9a-f]{64}',request_body_sha256)):
            self.fail('request_binding')
        self.active=True;self.request_hash=request_body_sha256
        self.stream_bytes=0;self.stream_hash=hashlib.sha256()

    def feed(self, data):
        self.poll()
        if self.closed or not isinstance(data,bytes) or len(data)>MAX_LINE:
            self.fail('pipe_input_bound')
        # Work per call is bounded; retain at most one incomplete frame.
        data=self.pending+data;self.pending=b''
        try:
            while b'\n' in data:
                line,data=data.split(b'\n',1)
                if len(line)+1>MAX_LINE:self.fail('frame_bound')
                self.frame(strict_json(line))
            if len(data)>=MAX_LINE:self.fail('frame_bound')
            self.pending=data
        except (ValueError,TypeError,KeyError,RecursionError,UnicodeError,binascii.Error):
            self.fail('invalid_or_unverified_frame')

    def frame(self, value):
        if (not self.active or not isinstance(value,dict) or type(value.get('schema_version')) is not int
                or value['schema_version']!=1 or type(value.get('id')) is not int
                or value['id']!=self.next_id):
            self.fail('frame_identity')
        if value.get('kind')=='chunk':
            if set(value)!={'schema_version','id','kind','data'} or not isinstance(value['data'],str):
                self.fail('chunk_shape')
            raw=base64.b64decode(value['data'],validate=True)
            if not 1<=len(raw)<=MAX_CHUNK or self.stream_bytes+len(raw)>MAX_STREAM:
                self.fail('stream_bound')
            self.stream_bytes+=len(raw);self.stream_hash.update(raw)
            if self.on_chunk is not None:
                try:self.on_chunk(raw)
                except Exception:self.fail('worker_write_unknown')
            return
        if value.get('kind')!='terminal':self.fail('frame_kind')
        if value.get('complete') is not True or value.get('usage') is None:
            self.fail('terminal_usage_unknown')
        if (value.get('request_body_sha256')!=self.request_hash or self.stream_bytes==0
                or value.get('unit')!=UNIT or value.get('provider_request_started') is not True
                or type(value.get('upstream_http_status')) is not int or value['upstream_http_status']!=200
                or value.get('provider_billing_stop_proven') is not False):
            self.fail('terminal_binding')
        actual=value.get('observed_response_model')
        if not isinstance(actual,str) or not (actual==self.model or actual.startswith(self.model+'-')):
            self.fail('model_binding')
        rid=value.get('response_id')
        if not isinstance(rid,str) or not 1<=len(rid)<=256 or rid in self.response_ids:
            self.fail('duplicate_or_missing_response')
        usage=value['usage']
        keys=set(self.totals)|{'reasoning_output_tokens'}
        if not isinstance(usage,dict) or set(usage)!=keys:self.fail('usage_shape')
        if any(not integer(usage[k]) for k in self.totals):self.fail('usage_type')
        inp,cached,out,total=(usage[k] for k in ('input_tokens','cached_input_tokens','output_tokens','total_tokens'))
        reasoning=usage['reasoning_output_tokens']
        if (total==0 or total!=inp+out or cached>inp
                or reasoning is not None and (not integer(reasoning) or reasoning>out)):
            self.fail('usage_incoherent')
        expected={k:self.totals[k]+usage[k] for k in self.totals}
        cumulative=value.get('cumulative_usage')
        if (not isinstance(cumulative,dict) or set(cumulative)!=set(expected)
                or any(not integer(cumulative[k],64_000_000_000) or cumulative[k]!=expected[k] for k in expected)):
            self.fail('cumulative_usage')
        i,c,o=RATES[self.model]
        nano=self.nano_credits+(inp-cached)*i+cached*c+out*o
        cost=value.get('cumulative_equivalent_microcredits')
        if not integer(cost,10**18) or cost!=(nano+999)//1000:self.fail('cumulative_cost')
        reason=value.get('stop_reason')
        if reason not in (None,'observed_token_limit','response_count_limit'):
            self.fail('terminal_stop_unknown')
        self.totals=expected;self.nano_credits=nano;self.response_ids.add(rid)
        self.last_receipt={'assignment_id':self.assignment_id,'request_id':self.next_id,
                           'request_body_sha256':self.request_hash,'response_id':rid,
                           'model':self.model,'effort':self.effort,'observed_response_model':actual,
                           'sse_sha256':self.stream_hash.hexdigest(),'sse_bytes':self.stream_bytes,
                           'usage':copy.deepcopy(usage),'cumulative_usage':dict(expected),
                           'observed_equivalent_microcredits':cost,'unit':UNIT,'authority':False}
        self.terminals+=1;self.next_id+=1;self.active=False
        if reason is not None or expected['total_tokens']>=6000 or self.next_id>64:
            self.stopped=True;self.reason=reason or 'observed_local_limit'

    def eof(self):
        self.closed=True
        if self.pending or self.active or not self.terminals:self.fail('pipe_eof_unknown')

    def receipt(self):
        return copy.deepcopy(self.last_receipt)

    def report(self):
        return {'assignment_id':self.assignment_id,'model':self.model,'effort':self.effort,
                'verified_terminals':self.terminals,'observed_tokens':self.totals['total_tokens'],
                'observed_equivalent_microcredits':(self.nano_credits+999)//1000,
                'usage_unknown':self.unknown,'stop_required':self.stopped,'stop_reason':self.reason,
                'cleanup_required':True,'cleanup_verified':False,
                'runtime_accounting':'MISSING_INTEGRATION','execution_authorized':False,
                'allow_next_trial':False,'provider_billing_stop_proven':False,
                'actual_subscription_debit':None}


def observe_fd(fd, observer):
    """Consume a caller-owned pipe only; never launch, own or close its processes.

    A synchronous worker sink must be bounded/nonblocking by its Runtime owner.
    This function cannot interrupt a blocked callback or prove writer cleanup.
    """
    try:
        metadata=os.fstat(fd)
        anonymous=(metadata.st_nlink==0 or sys.platform=='linux' and
                   re.fullmatch(r'pipe:\[\d+\]',os.readlink('/proc/self/fd/'+str(fd))) is not None)
        if not stat.S_ISFIFO(metadata.st_mode) or metadata.st_uid!=os.getuid() or not anonymous:
            observer.fail('private_pipe_required')
        while True:
            observer.poll()
            remaining=max(0,min(.1,observer.deadline-observer.clock()))
            ready,_,_=select.select([fd],[],[],remaining)
            if not ready:continue
            raw=os.read(fd,65536)
            if not raw:
                observer.eof();return observer.report()
            observer.feed(raw)
            if observer.stopped:return observer.report()
    except (OSError,ValueError):
        observer.fail('pipe_transport_or_observation_unknown')


def budget_check(assignments):
    """Read-only arithmetic. Neither a ledger nor a spend authorization."""
    if not isinstance(assignments,list) or len(assignments)!=24:raise ValueError('24_unique_cells_required')
    ids=set();counts=dict.fromkeys(RATES,0);subset=0
    expected={(task,arm,condition) for task in ('t02','t05','t08')
              for arm in ('Lm','Lx','Am','Ax') for condition in ('H0','H1')}
    cells=set()
    for row in assignments:
        aid=row['assignment_id'];model=row['model'];arm=row['arm']
        cell=(row['task_id'],arm,row['condition'])
        if (aid in ids or model not in RATES or cell not in expected or cell in cells
                or row['status']!='not_run' or row['repetition']!=1 or row['attempt']!=1
                or aid!='-'.join((row['task_id'],arm,'r1',row['condition']))
                or model!=('gpt-6-luna' if arm[0]=='L' else 'gpt-6-astra')
                or row['effort']!=('medium' if arm[1]=='m' else 'xhigh')):
            raise ValueError('matrix_binding')
        ids.add(aid);cells.add(cell);counts[model]+=1
        subset+=row['task_id']=='t08' and row['effort']=='medium'
    conditional=sum(counts[m]*6000*RATES[m][2] for m in RATES)//1000
    scenarios=[sum(counts[m]*(inp*RATES[m][0]+out*RATES[m][2]) for m in RATES)//1000
               for inp,out in ((3000,1000),(4500,1500))]
    return {'assignments':24,'medium_t08_subset':subset,'model_counts':counts,
            'scenarios_microcredits':scenarios,'candidate_observed_stop_microcredits':48000000,
            'conditional_bound_microcredits':conditional,'hard_provider_cap':False,
            'actual_subscription_debit':None,'execution_authorized':False,
            'assumption':'6000 cumulative billed tokens/cell, all output, Standard, no overrun or extra charges',
            'rate_card_date':'2026-10-15','current_rate_card_revalidated':False}
