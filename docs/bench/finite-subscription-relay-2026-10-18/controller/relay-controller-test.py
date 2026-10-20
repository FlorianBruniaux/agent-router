"""Offline tests. Removing a bound, binding check or stop must break these cases."""
import base64
import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
import threading

ROOT = Path(__file__).resolve().parent
sys.path.insert(0, str(ROOT))
import subscription_gateway as gateway
SPEC = importlib.util.spec_from_file_location('relay_controller', ROOT / 'relay-controller-offline.py')
controller = None
if Path(SPEC.origin).exists():
    controller = importlib.util.module_from_spec(SPEC)
    SPEC.loader.exec_module(controller)


def request():
    return json.loads(Path(str(ROOT/'relay-controller-native-request.json')).read_text())


def wire(rid='resp_1', inp=1000, out=500):
    response = {'id': rid, 'model': 'gpt-6-luna', 'status': 'completed', 'output': [],
                'usage': {'input_tokens': inp, 'output_tokens': out, 'total_tokens': inp+out,
                          'input_tokens_details': {'cached_tokens': min(200,inp)},
                          'output_tokens_details': {'reasoning_tokens': min(100,out)}}}
    return b'data: '+json.dumps({'type': 'response.completed', 'response': response}).encode()+b'\n\n'


def frames(rid='resp_1', inp=1000, out=500, clock=lambda: 0):
    g = gateway.Gateway({'model':'gpt-6-luna','effort':'medium'},
                        lambda raw, remaining: (200, 'text/event-stream', iter((wire(rid,inp,out),))), clock=clock)
    return list(g.handle({'schema_version':1,'id':1,'body':request()}))


def encoded(rows):
    return b''.join(json.dumps(x,separators=(',',':')).encode()+b'\n' for x in rows)


class ControllerTests(unittest.TestCase):
    def setUp(self):
        self.assertIsNotNone(controller, 'Missing offline private-pipe controller')
        self.now = [0.0]
        self.delivered = []
        self.o = controller.PipeObserver('t08-Lm-r1-H1','gpt-6-luna','medium',
                                         started_at=0, clock=lambda:self.now[0], on_chunk=self.delivered.append)
        raw = json.dumps(request(),ensure_ascii=False,separators=(',',':')).encode()
        self.digest = hashlib.sha256(raw).hexdigest()
        self.o.begin(1,self.digest)

    def test_packaged_request_is_explicitly_synthetic_without_session_metadata(self):
        import re
        value=request()
        self.assertIn('SYNTHETIC PUBLIC FIXTURE',value['instructions'])
        self.assertNotIn('prompt_cache_key',value)
        self.assertNotIn('client_metadata',value)
        self.assertEqual(value['input'][0],{'role':'user','content':'Synthetic public offline request. R2-LOCAL-TOOLS-V1'})
        self.assertEqual(value['input'][1]['call_id'],'call_synthetic_read_1')
        self.assertIsNone(re.search(r'[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}',json.dumps(value)))

    def test_packaged_intent_is_synthetic_and_has_no_historical_authority(self):
        import re
        value=json.loads((ROOT/'launch-intent.json').read_text())
        self.assertIs(value.get('qualification_only'),True)
        self.assertIs(value.get('synthetic'),True)
        self.assertIsNone(value['budget_approved'])
        self.assertEqual(value['account_sha256'],'b'*64)
        self.assertEqual(set(value),{'schema_version','scope','qualification_only','synthetic',
                                   'account_sha256','budget_approved','unmodified_experiment'})
        self.assertIsNone(re.search(r'[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}',json.dumps(value)))

    def test_real_gateway_frames_fragmented_preserve_bytes_and_never_admit_dispatch(self):
        data = encoded(frames())
        for i in range(0,len(data),7): self.o.feed(data[i:i+7])
        self.o.eof()
        r = self.o.report()
        self.assertEqual(b''.join(self.delivered),wire())
        self.assertEqual(r['observed_equivalent_microcredits'],8300)
        self.assertEqual(r['observed_tokens'],1500)
        self.assertEqual(r['verified_terminals'],1)
        self.assertFalse(r['execution_authorized'])
        self.assertFalse(r['allow_next_trial'])
        self.assertEqual(r['runtime_accounting'],'MISSING_INTEGRATION')

    def test_unknown_after_known_keeps_known_charge_and_blocks(self):
        self.o.feed(encoded(frames()))
        self.o.begin(2,self.digest)
        bad = frames('resp_2')[-1]; bad.update(id=2,complete=False,usage=None)
        with self.assertRaises(ValueError):self.o.feed(encoded([bad]))
        r=self.o.report()
        self.assertTrue(r['stop_required'])
        self.assertTrue(r['usage_unknown'])
        self.assertEqual(r['observed_equivalent_microcredits'],8300)
        with self.assertRaises(ValueError): self.o.begin(3,self.digest)

    def test_duplicate_terminal_and_response_id_cannot_be_counted_twice(self):
        data=frames(); self.o.feed(encoded(data))
        with self.assertRaises(ValueError):self.o.feed(encoded([data[-1]]))
        self.assertEqual(self.o.report()['verified_terminals'],1)

    def test_repeated_response_id_across_requests_refuses(self):
        self.o.feed(encoded(frames()))
        self.o.begin(2,self.digest)
        data=frames()
        for x in data:x['id']=2
        with self.assertRaises(ValueError):self.o.feed(encoded(data))
        self.assertEqual(self.o.report()['observed_tokens'],1500)

    def test_bindings_counters_and_credit_unit_are_checked(self):
        changes=[{'id':2},{'schema_version':True},{'request_body_sha256':'0'*64},
                 {'observed_response_model':'gpt-6-astra'},{'unit':'USD'},
                 {'cumulative_equivalent_microcredits':0},{'cumulative_usage':{'total_tokens':1}},
                 {'provider_request_started':False},{'upstream_http_status':True},
                 {'provider_billing_stop_proven':True}]
        for change in changes:
            with self.subTest(change=change):
                o=controller.PipeObserver('t08-Lm-r1-H1','gpt-6-luna','medium',started_at=0,clock=lambda:0)
                o.begin(1,self.digest); data=frames();data[-1].update(change)
                with self.assertRaises(ValueError):o.feed(encoded(data))
                self.assertTrue(o.report()['stop_required'])
                self.assertEqual(o.report()['verified_terminals'],0)

    def test_usage_types_cache_reasoning_and_total_are_checked(self):
        for change in ({'input_tokens':True},{'cached_input_tokens':1001},
                       {'reasoning_output_tokens':501},{'total_tokens':0},{'total_tokens':1501}):
            with self.subTest(change=change):
                o=controller.PipeObserver('t08-Lm-r1-H1','gpt-6-luna','medium',started_at=0,clock=lambda:0)
                o.begin(1,self.digest); data=frames();data[-1]['usage'].update(change)
                with self.assertRaises(ValueError):o.feed(encoded(data))
                self.assertTrue(o.report()['usage_unknown'])

    def test_json_duplicates_base64_and_frame_bounds_refuse_without_forwarding(self):
        for raw in (b'{"id":1,"id":1}\n', b'{"a":NaN}\n',b'x'*300001,
                    encoded([{'schema_version':1,'id':1,'kind':'chunk','data':'!'}]),
                    encoded([{'schema_version':1,'id':1,'kind':'chunk','data':base64.b64encode(b'x'*65537).decode()}])):
            with self.subTest(size=len(raw)):
                o=controller.PipeObserver('t08-Lm-r1-H1','gpt-6-luna','medium',started_at=0,clock=lambda:0)
                o.begin(1,self.digest)
                with self.assertRaises(ValueError):o.feed(raw)
                self.assertTrue(o.report()['stop_required'])

    def test_stream_overflow_is_bounded(self):
        chunk=encoded([{'schema_version':1,'id':1,'kind':'chunk','data':base64.b64encode(b'x'*65536).decode()}])
        for _ in range(128):self.o.feed(chunk)
        with self.assertRaises(ValueError):self.o.feed(chunk)
        self.assertTrue(self.o.report()['stop_required'])

    def test_idle_silent_and_mid_frame_deadlines_stop(self):
        self.now[0]=59.9;self.o.poll();self.assertFalse(self.o.report()['stop_required'])
        self.o.feed(b'{')
        self.now[0]=60
        with self.assertRaises(ValueError):self.o.poll()
        self.assertTrue(self.o.report()['usage_unknown'])
        self.assertTrue(self.o.report()['cleanup_required'])

    def test_eof_mid_request_or_mid_line_cannot_complete_trial(self):
        for partial in (b'',b'{'):
            with self.subTest(partial=partial):
                o=controller.PipeObserver('t08-Lm-r1-H1','gpt-6-luna','medium',started_at=0,clock=lambda:0)
                o.begin(1,self.digest);o.feed(partial)
                with self.assertRaises(ValueError):o.eof()
                self.assertTrue(o.report()['stop_required'])

    def test_observed_6000_tokens_stops_campaign_and_preserves_charge(self):
        self.o.feed(encoded(frames(inp=5000,out=1000)))
        r=self.o.report()
        self.assertTrue(r['stop_required']);self.assertFalse(r['usage_unknown'])
        self.assertEqual(r['observed_tokens'],6000)
        with self.assertRaises(ValueError):self.o.begin(2,self.digest)

    def test_skipped_id_and_concurrent_request_refuse(self):
        with self.assertRaises(ValueError):self.o.begin(2,self.digest)
        self.assertTrue(self.o.report()['stop_required'])

    def test_two_gateway_rounds_check_cumulative_not_only_last(self):
        counter=[0]
        def send(raw,remaining):
            counter[0]+=1
            return 200,'text/event-stream',iter((wire('resp_'+str(counter[0])),))
        g=gateway.Gateway({'model':'gpt-6-luna','effort':'medium'},send,clock=lambda:0)
        self.o.feed(encoded(list(g.handle({'schema_version':1,'id':1,'body':request()}))))
        self.o.begin(2,self.digest)
        self.o.feed(encoded(list(g.handle({'schema_version':1,'id':2,'body':request()}))))
        self.assertEqual(self.o.report()['observed_tokens'],3000)
        self.assertEqual(self.o.report()['observed_equivalent_microcredits'],16600)

    def test_owned_anonymous_pipe_and_regular_file_refusal(self):
        read,write=os.pipe()
        try:
            data=encoded(frames());writer_fd=write;write=None;errors=[]
            def provide():
                try:
                    offset=0
                    while offset<len(data):offset+=os.write(writer_fd,data[offset:])
                except OSError as error:errors.append(type(error).__name__)
                finally:os.close(writer_fd)
            writer=threading.Thread(target=provide,daemon=True);writer.start()
            controller.observe_fd(read,self.o)
            writer.join(2);self.assertFalse(writer.is_alive());self.assertEqual(errors,[])
            self.assertEqual(self.o.report()['verified_terminals'],1)
        finally:
            os.close(read)
            if write is not None:os.close(write)
        with open(__file__,'rb') as f:
            with self.assertRaises(ValueError):controller.observe_fd(f.fileno(),self.o)

    def test_silent_pipe_deadline_needs_cleanup_no_process_spawned(self):
        read,write=os.pipe()
        try:
            o=controller.PipeObserver('t08-Lm-r1-H1','gpt-6-luna','medium',started_at=0,clock=lambda:60)
            with self.assertRaises(ValueError):controller.observe_fd(read,o)
            self.assertTrue(o.report()['cleanup_required'])
        finally:os.close(read);os.close(write)

    def test_closed_descriptor_marks_unknown_and_never_authorizes(self):
        read,write=os.pipe();os.close(read);os.close(write)
        with self.assertRaises(ValueError):controller.observe_fd(read,self.o)
        self.assertTrue(self.o.report()['usage_unknown'])
        self.assertTrue(self.o.report()['stop_required'])

    def test_terminal_receipt_contains_request_identity_and_stream_hash(self):
        self.o.feed(encoded(frames()))
        receipt=self.o.receipt()
        self.assertEqual(receipt['assignment_id'],'t08-Lm-r1-H1')
        self.assertEqual(receipt['request_id'],1)
        self.assertEqual(receipt['request_body_sha256'],self.digest)
        self.assertEqual(receipt['response_id'],'resp_1')
        self.assertEqual(receipt['sse_sha256'],hashlib.sha256(wire()).hexdigest())
        self.assertEqual(receipt['observed_equivalent_microcredits'],8300)
        self.assertFalse(receipt['authority'])
        receipt['usage']['total_tokens']=0
        self.assertEqual(self.o.receipt()['usage']['total_tokens'],1500)

    def test_64_responses_refuse_a_65th_request(self):
        n=[0]
        def send(raw,remaining):
            n[0]+=1
            return 200,'text/event-stream',iter((wire('resp_'+str(n[0]),1,1),))
        g=gateway.Gateway({'model':'gpt-6-luna','effort':'medium'},send,clock=lambda:0)
        for i in range(1,65):
            if i>1:self.o.begin(i,self.digest)
            self.o.feed(encoded(list(g.handle({'schema_version':1,'id':i,'body':request()}))))
        self.assertEqual(self.o.report()['verified_terminals'],64)
        self.assertEqual(self.o.report()['observed_tokens'],128)
        self.assertTrue(self.o.report()['stop_required'])
        with self.assertRaises(ValueError):self.o.begin(65,self.digest)

    def test_named_fifo_is_not_a_private_inherited_pipe(self):
        with tempfile.TemporaryDirectory(prefix='relay-controller-test-') as tmp:
            path=Path(tmp)/'named';os.mkfifo(path,0o600)
            fd=os.open(path,os.O_RDONLY|os.O_NONBLOCK)
            try:
                with self.assertRaises(ValueError):controller.observe_fd(fd,self.o)
                self.assertTrue(self.o.report()['stop_required'])
            finally:os.close(fd)

    def test_worker_write_failure_is_unknown_and_has_no_terminal_receipt(self):
        def blocked_sink(raw):raise OSError('synthetic-private-detail')
        o=controller.PipeObserver('t08-Lm-r1-H1','gpt-6-luna','medium',
                                  started_at=0,clock=lambda:0,on_chunk=blocked_sink)
        o.begin(1,self.digest)
        with self.assertRaises(ValueError):o.feed(encoded(frames()))
        self.assertTrue(o.report()['usage_unknown'])
        self.assertIsNone(o.receipt())
        self.assertNotIn('synthetic-private-detail',json.dumps(o.report()))

    def test_24_budget_includes_four_medium_and_has_no_hard_cap(self):
        intent=json.loads((ROOT/'launch-intent.json').read_text())
        b=controller.budget_check(intent['unmodified_experiment']['assignments'])
        self.assertEqual(b['assignments'],24)
        self.assertEqual(b['medium_t08_subset'],4)
        self.assertEqual(b['conditional_bound_microcredits'],90900000)
        self.assertEqual(b['candidate_observed_stop_microcredits'],48000000)
        self.assertEqual(b['scenarios_microcredits'],[24240000,36360000])
        self.assertFalse(b['hard_provider_cap'])
        self.assertIsNone(b['actual_subscription_debit'])
        rows=intent['unmodified_experiment']['assignments']
        with self.assertRaises(ValueError):controller.budget_check(rows+rows[:4])

if __name__=='__main__':unittest.main()
