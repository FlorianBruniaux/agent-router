"""Composed offline tests: private Unix sockets, coded SSE, one temporary results file."""
import base64
import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import socket
import sys
import tempfile
import threading
import time
import unittest

ROOT=Path(__file__).resolve().parent
sys.path.insert(0,str(ROOT))

def load(name,file):
    spec=importlib.util.spec_from_file_location(name,ROOT/file)
    value=importlib.util.module_from_spec(spec);spec.loader.exec_module(value);return value

fixtures=load('session_results_fixture','relay-controller-results-test.py')
module=None
if (ROOT/'relay-controller-session.py').exists():module=load('host_session','relay-controller-session.py')


class SessionTests(unittest.TestCase):
    def setUp(self):
        self.assertIsNotNone(module,'Missing composed private session controller')
        self.tmp=tempfile.TemporaryDirectory(prefix='relay-controller-results-test-');self.addCleanup(self.tmp.cleanup)
        config=fixtures.configuration()
        self.manager=fixtures.manager.ResultsManager(Path(self.tmp.name),fixtures.MANIFEST,fixtures.TEMPLATE,
            budget_microcredits=48000000,configuration=config,verified_configuration_sha256=fixtures.digest(config))
        self.addCleanup(self.manager.close)
        self.binding={'assignment_id':fixtures.AID,'run_id':fixtures.RUN,'attempt':1,
            'approval_digest':fixtures.APPROVAL,'configuration_sha256':self.manager.config_sha,
            'account_sha256':config['account_sha256'],'controller_sha256':module.source_sha256(),
            'channel_instance_sha256':'d'*64,'nonce':'e'*64}
        self.host,self.runtime=socket.socketpair();self.addCleanup(self.host.close);self.addCleanup(self.runtime.close)
        self.calls=[]
        self.request=fixtures.fixture.request();self.request['reasoning']['effort']='xhigh'
        self.failure=[];self.result=[]

    def send(self,raw,remaining):
        self.calls.append(raw)
        return 200,'text/event-stream',iter((fixtures.fixture.wire('resp_'+str(len(self.calls))),))

    def start(self,send=None,seconds=2):
        channel=module.PrivateRuntimeIO(self.host.fileno(),self.host.fileno(),timeout=seconds,borrow=True)
        session=module.SessionController(channel,self.manager,self.binding,self.send if send is None else send)
        def run():
            try:self.result.append(session.run())
            except Exception as error:self.failure.append(error)
        self.thread=threading.Thread(target=run,daemon=True);self.thread.start()
        self.addCleanup(lambda:self.thread.join(3))
        return session

    def put(self,value):self.runtime.sendall(json.dumps(value,separators=(',',':')).encode()+b'\n')
    def get(self):
        self.runtime.settimeout(3);data=b''
        while not data.endswith(b'\n'):data+=self.runtime.recv(1)
        return json.loads(data)
    def ready(self,binding=None):
        self.put({'schema_version':1,'kind':'runtime_ready','binding':self.binding if binding is None else binding})
        return self.get()
    def complete(self,**changes):
        frame={'schema_version':1,'kind':'runtime_cleanup','binding':self.binding,
               'cleanup':'namespace_terminated','native_status':'completed','proof_sha256':'c'*64}
        frame.update(changes);self.put(frame);return self.get()
    def request_once(self):
        self.put({'schema_version':1,'id':len(self.calls)+1,'body':self.request})
        rows=[]
        while not rows or rows[-1]['kind']!='terminal':rows.append(self.get())
        self.checkpoint=self.get()
        self.assertEqual(self.checkpoint['kind'],'controller_request_finished')
        return rows
    def joined(self):
        self.thread.join(3);self.assertFalse(self.thread.is_alive())

    def test_ready_ack_persists_intent_before_worker_and_forwards_sse_unchanged(self):
        self.start();ack=self.ready()
        self.assertEqual(ack['kind'],'controller_ready')
        self.assertEqual(self.manager.snapshot()['rows'][0]['status'],'in_progress')
        self.assertEqual(self.calls,[])
        rows=self.request_once()
        raw=b''.join(base64.b64decode(x['data']) for x in rows if x['kind']=='chunk')
        self.assertEqual(raw,fixtures.fixture.wire('resp_1'))
        ack=self.complete();self.joined()
        self.assertEqual(ack['kind'],'controller_finished')
        self.assertFalse(ack['execution_authorized'])
        self.assertEqual(self.failure,[])
        self.assertEqual(self.manager.observation()['known_cost_microcredits'],8300)
        self.assertTrue(self.manager.observation()['budget_observation_allows_next'])
        self.assertEqual(len(list(Path(self.tmp.name).iterdir())),2)

    def test_wrong_nonce_config_account_or_controller_never_sends_or_starts(self):
        self.start();bad={**self.binding,'nonce':'f'*64}
        self.put({'schema_version':1,'kind':'runtime_ready','binding':bad})
        self.joined();self.assertEqual(self.calls,[])
        self.assertEqual(self.manager.snapshot()['rows'][0]['status'],'not_run')
        self.assertTrue(self.failure)

    def test_request_before_ready_is_refused_without_provider(self):
        self.start();self.put({'schema_version':1,'id':1,'body':self.request});self.joined()
        self.assertEqual(self.calls,[]);self.assertTrue(self.failure)
        self.assertEqual(self.manager.snapshot()['rows'][0]['status'],'not_run')

    def test_complete_json_from_wrong_session_cannot_confirm_cleanup(self):
        self.start();self.ready();self.request_once()
        self.put({'schema_version':1,'kind':'runtime_cleanup','binding':{**self.binding,'run_id':'other'},
                  'cleanup':'namespace_terminated','native_status':'completed','proof_sha256':'c'*64})
        self.joined()
        self.assertTrue(self.failure)
        self.assertFalse(self.manager.observation()['budget_observation_allows_next'])
        self.assertEqual(self.manager.observation()['known_cost_microcredits'],8300)

    def test_unknown_cleanup_blocks_without_forgetting_charge(self):
        self.start();self.ready();self.request_once();self.complete(cleanup='unknown',proof_sha256=None)
        self.joined();self.assertEqual(self.failure,[])
        self.assertFalse(self.manager.observation()['budget_observation_allows_next'])
        self.assertEqual(self.manager.observation()['known_cost_microcredits'],8300)

    def test_unknown_usage_is_never_committed_as_zero_and_requests_stop(self):
        self.start(send=lambda raw,remaining:(200,'text/event-stream',iter((b'data: {"type":"response.completed","response":{}}\n\n',))))
        self.ready();rows=self.request_once()
        self.assertFalse(rows[-1]['complete']);self.assertFalse(self.checkpoint['continue'])
        self.complete(cleanup='unknown',proof_sha256=None,native_status='interrupted');self.joined()
        self.assertTrue(self.manager.observation()['usage_unknown'])
        self.assertIsNone(self.manager.snapshot()['rows'][0]['cost_microcredits'])
        self.assertFalse(self.manager.observation()['budget_observation_allows_next'])

    def test_pipe_eof_after_known_usage_keeps_charge_and_blocks(self):
        self.start();self.ready();self.request_once();self.runtime.shutdown(socket.SHUT_WR);self.joined()
        self.assertEqual(self.manager.observation()['known_cost_microcredits'],8300)
        self.assertTrue(self.manager.observation()['usage_unknown']);self.assertTrue(self.failure)

    def test_silent_runtime_deadline_is_bounded(self):
        started=time.monotonic();self.start(seconds=.1);self.joined()
        self.assertLess(time.monotonic()-started,1.5);self.assertTrue(self.failure)
        self.assertEqual(self.calls,[])

    def test_blocked_delivery_times_out_without_caller_buffer_growth(self):
        self.host.setsockopt(socket.SOL_SOCKET,socket.SO_SNDBUF,1024)
        channel=module.PrivateRuntimeIO(self.host.fileno(),self.host.fileno(),timeout=.05,borrow=True)
        with self.assertRaises((ValueError,TimeoutError,OSError)):
            for _ in range(100):channel.write({'x':'a'*100000})

    def test_frame_bound_and_duplicate_json_keys_fail_without_provider(self):
        self.start();self.runtime.sendall(b'{"schema_version":1,"kind":"runtime_ready","kind":"runtime_ready"}\n')
        self.joined();self.assertEqual(self.calls,[]);self.assertTrue(self.failure)

    def test_two_tool_rounds_keep_cumulative_cost_once(self):
        self.start();self.ready();self.request_once();self.assertTrue(self.checkpoint['continue'])
        self.request_once();self.assertTrue(self.checkpoint['continue'])
        self.complete();self.joined()
        self.assertEqual(self.manager.observation()['known_cost_microcredits'],16600)
        self.assertEqual(len(self.calls),2)

    def test_unapproved_response_snapshot_is_unknown_before_cost_ingestion(self):
        def variant(raw,remaining):
            wire=fixtures.fixture.wire().replace(b'gpt-6-luna',b'gpt-6-luna-unapproved')
            return 200,'text/event-stream',iter((wire,))
        self.start(send=variant);self.ready();self.request_once()
        self.assertFalse(self.checkpoint['continue'])
        self.assertTrue(self.manager.observation()['usage_unknown'])
        self.assertIsNone(self.manager.snapshot()['rows'][0]['cost_microcredits'])
        self.complete(native_status='interrupted');self.joined()

    def test_observed_campaign_budget_stops_at_barrier_and_keeps_cleanup_channel(self):
        tmp=tempfile.TemporaryDirectory(prefix='relay-controller-results-test-');self.addCleanup(tmp.cleanup)
        config=fixtures.configuration(8000)
        self.manager=fixtures.manager.ResultsManager(Path(tmp.name),fixtures.MANIFEST,fixtures.TEMPLATE,
            budget_microcredits=8000,configuration=config,verified_configuration_sha256=fixtures.digest(config))
        self.addCleanup(self.manager.close)
        self.binding['configuration_sha256']=self.manager.config_sha
        self.start();self.ready();self.request_once()
        self.assertIsNone(self.checkpoint.get('stop_reason'))
        self.assertFalse(self.checkpoint['continue'])
        self.assertEqual(self.checkpoint['reason'],'observed_campaign_budget')
        self.complete(native_status='interrupted');self.joined()
        self.assertEqual(self.failure,[])
        self.assertEqual(self.manager.observation()['known_cost_microcredits'],8300)
        self.assertFalse(self.manager.observation()['budget_observation_allows_next'])

    def test_duplicate_request_sends_stop_without_another_provider_call(self):
        self.start();self.ready();self.request_once()
        self.put({'schema_version':1,'id':1,'body':self.request})
        stop=self.get();self.assertEqual(stop['kind'],'controller_stop')
        self.complete(native_status='interrupted');self.joined()
        self.assertEqual(len(self.calls),1)
        self.assertEqual(self.manager.observation()['known_cost_microcredits'],8300)
        self.assertFalse(self.manager.observation()['budget_observation_allows_next'])

    def test_cleanup_without_any_request_cannot_complete_the_trial(self):
        self.start();self.ready();self.complete();self.joined()
        self.assertFalse(self.manager.observation()['budget_observation_allows_next'])
        self.assertTrue(self.manager.observation()['usage_unknown'])

    def test_regular_file_cannot_be_runtime_channel(self):
        with open(__file__,'rb') as stream:
            with self.assertRaises(ValueError):module.PrivateRuntimeIO(stream.fileno(),stream.fileno(),borrow=True)

if __name__=='__main__':unittest.main()
