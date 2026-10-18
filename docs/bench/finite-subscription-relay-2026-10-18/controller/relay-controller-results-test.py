"""Offline result-state tests: no transport or model is started."""
import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT=Path(__file__).resolve().parent
sys.path.insert(0,str(ROOT))
CS=importlib.util.spec_from_file_location('pipe_controller',ROOT/'relay-controller-offline.py')
pipe=importlib.util.module_from_spec(CS);CS.loader.exec_module(pipe)
TS=importlib.util.spec_from_file_location('pipe_fixture',ROOT/'relay-controller-test.py')
fixture=importlib.util.module_from_spec(TS);TS.loader.exec_module(fixture)
SPEC=importlib.util.spec_from_file_location('campaign_results',ROOT/'relay-controller-results.py')
manager=None
if Path(SPEC.origin).exists():
    manager=importlib.util.module_from_spec(SPEC);SPEC.loader.exec_module(manager)
SEALED=ROOT/'sealed-inputs'
MANIFEST=(SEALED/'manifest-24-preparation.json').read_bytes()
TEMPLATE=(SEALED/'results.template.json').read_bytes()
AID='t02-Lx-r1-H0'
RUN='fixture-run-1';APPROVAL='b'*64


def configuration(budget=48000000):
    return {'profile':'codex_subscription_relay','manifest_sha256':hashlib.sha256(MANIFEST).hexdigest(),
            'template_sha256':hashlib.sha256(TEMPLATE).hexdigest(),
            'controller_sha256':hashlib.sha256((ROOT/'relay-controller-offline.py').read_bytes()).hexdigest(),
            'gateway_sha256':hashlib.sha256((ROOT/'subscription_gateway.py').read_bytes()).hexdigest(),
            'account_sha256':'a'*64,'observed_campaign_budget_microcredits':budget,
            'observed_tokens_per_trial':6000,'wall_seconds_per_trial':60}


def digest(value):
    return hashlib.sha256(json.dumps(value,sort_keys=True,separators=(',',':')).encode()).hexdigest()


def receipt(number=1, inp=1000, out=500):
    # Actual PipeObserver, real gateway, coded responses only. First sealed cell is Lx.
    body=fixture.request();body['reasoning']['effort']='xhigh'
    raw=json.dumps(body,ensure_ascii=False,separators=(',',':')).encode()
    request_hash=hashlib.sha256(raw).hexdigest()
    n=[0]
    def send(raw,remaining):
        n[0]+=1
        return 200,'text/event-stream',iter((fixture.wire('resp_'+str(n[0]),inp,out),))
    g=fixture.gateway.Gateway({'model':'gpt-6-luna','effort':'xhigh'},send,clock=lambda:0)
    observer=pipe.PipeObserver(AID,'gpt-6-luna','xhigh',started_at=0,clock=lambda:0)
    for i in range(1,number+1):
        observer.begin(i,request_hash)
        observer.feed(fixture.encoded(list(g.handle({'schema_version':1,'id':i,'body':body}))))
    return observer.receipt()


def cleanup():
    return {'run_id':RUN,'approval_digest':APPROVAL,'proof_sha256':'c'*64,'writers_excluded':True}


class ResultsTests(unittest.TestCase):
    def setUp(self):
        self.assertIsNotNone(manager,'Missing bounded offline campaign results manager')
        self.tmp=tempfile.TemporaryDirectory(prefix='relay-controller-results-test-')
        self.addCleanup(self.tmp.cleanup)
        self.directory=Path(self.tmp.name);self.opened=[]
        self.addCleanup(lambda:[x.close() for x in self.opened])

    def open(self,budget=48000000,configuration_value=None,verified=None):
        config=configuration(budget) if configuration_value is None else configuration_value
        obj=manager.ResultsManager(self.directory,MANIFEST,TEMPLATE,budget_microcredits=budget,
                                   configuration=config,verified_configuration_sha256=digest(config) if verified is None else verified)
        self.opened.append(obj);return obj

    def start(self,budget=48000000):
        m=self.open(budget);m.begin_trial(AID,RUN,APPROVAL);return m

    def receive(self,m,r=None):
        r=receipt() if r is None else r
        m.before_request(AID,r['request_id'],r['request_body_sha256'])
        m.ingest(AID,r);return r

    def test_creation_requires_explicit_budget_and_correct_reviewed_digest_before_files(self):
        for budget,verified in ((None,'0'*64),(0,'0'*64),(True,'0'*64),(48000000,'0'*64)):
            with self.subTest(budget=budget):
                with self.assertRaises(ValueError):self.open(budget,verified=verified)
                self.assertEqual(list(self.directory.iterdir()),[])

    def test_creates_only_results_plus_lock_and_preserves_benchmark_format(self):
        m=self.open();state=m.snapshot()
        self.assertEqual(len(state['rows']),24)
        self.assertTrue(all(x['status']=='not_run' for x in state['rows']))
        self.assertEqual(set(self.directory.iterdir()),{self.directory/'results.json',self.directory/'results.lock'})
        self.assertEqual(state['model_calls_started'],0)
        self.assertIsNone(state['budget_approved'])
        self.assertFalse(state['execution_authorized'])
        self.assertTrue(m.observation()['budget_observation_allows_next'])
        self.assertFalse(m.observation()['execution_authorized'])

    def test_wrong_seal_or_changed_assignment_is_refused_without_state(self):
        altered=json.loads(TEMPLATE);altered['rows'][0]['effort']='medium'
        with self.assertRaises(ValueError):
            manager.ResultsManager(self.directory,MANIFEST,json.dumps(altered).encode(),budget_microcredits=48000000,
                                   configuration=configuration(),verified_configuration_sha256=digest(configuration()))
        self.assertEqual(list(self.directory.iterdir()),[])

    def test_exclusive_lock_blocks_second_manager_without_overwriting(self):
        m=self.open();before=(self.directory/'results.json').read_bytes()
        with self.assertRaises((ValueError,BlockingIOError)):self.open()
        self.assertEqual((self.directory/'results.json').read_bytes(),before)
        self.assertTrue(m.observation()['budget_observation_allows_next'])

    def test_before_dispatch_is_durable_and_second_attempt_is_refused(self):
        m=self.start();d=json.loads((self.directory/'results.json').read_text())
        self.assertEqual(d['rows'][0]['status'],'in_progress')
        self.assertEqual(d['rows'][0]['relay_results']['run_id'],RUN)
        self.assertFalse(m.observation()['budget_observation_allows_next'])
        with self.assertRaises(ValueError):m.begin_trial(AID,RUN,APPROVAL)

    def test_reopen_in_progress_stops_campaign_and_never_retries(self):
        m=self.start();m.close();other=self.open()
        self.assertEqual(other.snapshot()['rows'][0]['status'],'stopped')
        self.assertTrue(other.observation()['usage_unknown'])
        self.assertFalse(other.observation()['budget_observation_allows_next'])
        with self.assertRaises(ValueError):other.begin_trial(AID,RUN,APPROVAL)

    def test_two_receipts_use_last_cumulative_cost_without_double_counting(self):
        m=self.start();r=self.receive(m);m.ingest(AID,r)
        self.receive(m,receipt(2))
        row=m.snapshot()['rows'][0]
        self.assertEqual(row['cost_microcredits'],16600)
        self.assertEqual(row['usage']['total_tokens'],3000)
        self.assertEqual(m.observation()['known_cost_microcredits'],16600)
        self.assertEqual(len(row['relay_results']['receipts']),2)
        self.assertFalse(m.observation()['execution_authorized'])

    def test_missing_before_request_marker_blocks_ingestion(self):
        m=self.start()
        with self.assertRaises(ValueError):m.ingest(AID,receipt())
        self.assertFalse(m.observation()['budget_observation_allows_next'])

    def test_duplicate_id_with_changed_counter_or_hash_is_not_a_retry(self):
        for field in ('observed_equivalent_microcredits','request_body_sha256','response_id'):
            with self.subTest(field=field),tempfile.TemporaryDirectory(prefix='relay-controller-results-test-') as tmp:
                config=configuration()
                with manager.ResultsManager(Path(tmp),MANIFEST,TEMPLATE,budget_microcredits=48000000,
                        configuration=config,verified_configuration_sha256=digest(config)) as m:
                    m.begin_trial(AID,RUN,APPROVAL);r=self.receive(m)
                    bad=copy.deepcopy(r);bad[field]=0 if field=='observed_equivalent_microcredits' else 'd'*64
                    with self.assertRaises(ValueError):m.ingest(AID,bad)
                    self.assertEqual(m.observation()['known_cost_microcredits'],8300)
                    self.assertTrue(m.observation()['usage_unknown'])

    def test_unknown_preserves_known_prefix_and_blocks_future_cell(self):
        m=self.start();self.receive(m);m.mark_unknown(AID,'fixture_counter_missing')
        self.assertEqual(m.observation()['known_cost_microcredits'],8300)
        self.assertTrue(m.observation()['usage_unknown'])
        self.assertFalse(m.observation()['budget_observation_allows_next'])
        with self.assertRaises(ValueError):m.begin_trial('t02-Lx-r1-H1','fixture-run-2',APPROVAL)

    def test_threshold_6000_or_campaign_budget_blocks_even_with_cleanup(self):
        for budget,inp,out in ((8000,1000,500),(48000000,5000,1000)):
            with self.subTest(budget=budget),tempfile.TemporaryDirectory(prefix='relay-controller-results-test-') as tmp:
                config=configuration(budget)
                with manager.ResultsManager(Path(tmp),MANIFEST,TEMPLATE,budget_microcredits=budget,
                        configuration=config,verified_configuration_sha256=digest(config)) as m:
                    m.begin_trial(AID,RUN,APPROVAL);self.receive(m,receipt(inp=inp,out=out))
                    m.finish_trial(AID,'completed',cleanup())
                    self.assertFalse(m.observation()['budget_observation_allows_next'])
                    self.assertFalse(m.observation()['execution_authorized'])
                    self.assertGreater(m.observation()['known_cost_microcredits'],0)

    def test_success_requires_matching_cleanup_and_allows_only_next_sealed_cell(self):
        m=self.start();self.receive(m);m.finish_trial(AID,'completed',cleanup())
        self.assertTrue(m.observation()['budget_observation_allows_next'])
        m.close();m=self.open()
        self.assertTrue(m.observation()['budget_observation_allows_next'])
        m.begin_trial('t02-Lx-r1-H1','fixture-run-2',APPROVAL)
        self.assertEqual(m.snapshot()['rows'][1]['status'],'in_progress')

    def test_missing_or_wrong_cleanup_blocks_after_successful_usage(self):
        for proof in (None,{**cleanup(),'run_id':'other'},{**cleanup(),'writers_excluded':False}):
            with self.subTest(proof=proof),tempfile.TemporaryDirectory(prefix='relay-controller-results-test-') as tmp:
                config=configuration()
                with manager.ResultsManager(Path(tmp),MANIFEST,TEMPLATE,budget_microcredits=48000000,
                        configuration=config,verified_configuration_sha256=digest(config)) as m:
                    m.begin_trial(AID,RUN,APPROVAL);self.receive(m)
                    m.finish_trial(AID,'completed',proof)
                    self.assertFalse(m.observation()['budget_observation_allows_next'])
                    self.assertEqual(m.observation()['known_cost_microcredits'],8300)

    def test_failed_trial_cost_is_kept_and_no_followup(self):
        m=self.start();self.receive(m);m.finish_trial(AID,'failed',cleanup())
        self.assertEqual(m.observation()['known_cost_microcredits'],8300)
        self.assertFalse(m.observation()['budget_observation_allows_next'])
        self.assertEqual(m.snapshot()['rows'][0]['status'],'failed')

    def test_counter_modification_in_file_is_refused_and_never_repaired_implicitly(self):
        m=self.start();self.receive(m);m.close()
        path=self.directory/'results.json';d=json.loads(path.read_text());d['rows'][0]['cost_microcredits']=0
        path.write_text(json.dumps(d));before=path.read_bytes()
        with self.assertRaises(ValueError):self.open()
        self.assertEqual(path.read_bytes(),before)

    def test_incomplete_json_and_orphan_atomic_write_block_without_reset(self):
        path=self.directory/'results.json';path.write_bytes(b'{')
        with self.assertRaises(ValueError):self.open()
        self.assertEqual(path.read_bytes(),b'{')

    def test_failed_replace_keeps_previous_state_and_crash_orphan_blocks_reopen(self):
        m=self.open();path=self.directory/'results.json';before=path.read_bytes()
        with patch.object(manager.os,'replace',side_effect=OSError('synthetic_replace_error')):
            with self.assertRaises(OSError):m.begin_trial(AID,RUN,APPROVAL)
        self.assertEqual(path.read_bytes(),before)
        with self.assertRaises(ValueError):m.begin_trial(AID,RUN,APPROVAL)
        m.close()
        with self.assertRaises(ValueError):self.open()

    def test_fsync_failure_after_replace_cannot_be_reported_as_committed(self):
        m=self.open();original=os.fsync;count=[0]
        def sync(fd):
            count[0]+=1
            if count[0]==2:raise OSError('synthetic_directory_fsync_error')
            return original(fd)
        with patch.object(manager.os,'fsync',side_effect=sync):
            with self.assertRaises(OSError):m.begin_trial(AID,RUN,APPROVAL)
        m.close();m=self.open()
        self.assertFalse(m.observation()['budget_observation_allows_next'])
        self.assertEqual(m.snapshot()['rows'][0]['status'],'stopped')

    def test_external_file_change_while_lock_held_is_not_overwritten(self):
        m=self.open();path=self.directory/'results.json';path.write_text('{"foreign":true}')
        with self.assertRaises(ValueError):m.begin_trial(AID,RUN,APPROVAL)
        self.assertEqual(path.read_text(),'{"foreign":true}')

    def test_configuration_change_cannot_reopen_existing_state(self):
        m=self.open();m.close()
        with self.assertRaises(ValueError):self.open(96000000)

    def test_run_identity_cannot_be_reused_for_another_cell(self):
        m=self.start();self.receive(m);m.finish_trial(AID,'completed',cleanup())
        with self.assertRaises(ValueError):m.begin_trial('t02-Lx-r1-H1',RUN,APPROVAL)
        self.assertEqual(m.snapshot()['rows'][1]['status'],'not_run')

    def test_known_usage_survives_restart_after_pending_request_crash(self):
        m=self.start();self.receive(m)
        second=receipt(2);m.before_request(AID,2,second['request_body_sha256']);m.close()
        m=self.open()
        self.assertEqual(m.observation()['known_cost_microcredits'],8300)
        self.assertTrue(m.observation()['usage_unknown'])
        self.assertFalse(m.observation()['budget_observation_allows_next'])

    def test_self_checksum_is_not_enough_to_hide_a_modified_row_counter(self):
        m=self.start();self.receive(m);m.close()
        path=self.directory/'results.json';d=json.loads(path.read_text())
        d['rows'][0]['cost_microcredits']=0
        d['relay_results']['checksum']='';d['relay_results']['checksum']=digest(d)
        path.write_text(json.dumps(d));before=path.read_bytes()
        with self.assertRaises(ValueError):self.open()
        self.assertEqual(path.read_bytes(),before)

    def test_failed_first_atomic_replace_leaves_no_activatable_results(self):
        with patch.object(manager.os,'replace',side_effect=OSError('synthetic_initial_write_error')):
            with self.assertRaises(OSError):self.open()
        self.assertFalse((self.directory/'results.json').exists())
        with self.assertRaises(ValueError):self.open()

    def test_non_temporary_directory_and_hardlinked_results_are_refused(self):
        config=configuration()
        with self.assertRaises(ValueError):
            manager.ResultsManager(ROOT,MANIFEST,TEMPLATE,budget_microcredits=48000000,
                                   configuration=config,verified_configuration_sha256=digest(config))
        m=self.open();m.close();path=self.directory/'results.json'
        os.link(path,self.directory/'hardlink')
        with self.assertRaises(ValueError):self.open()

    def test_entire_24_cell_coded_campaign_uses_one_results_state(self):
        m=self.open();rows=copy.deepcopy(m.snapshot()['rows'])
        for index,row in enumerate(rows,1):
            rid='resp_'+row['assignment_id']
            body=fixture.request();body['model']=row['model'];body['reasoning']['effort']=row['effort']
            request_hash=hashlib.sha256(json.dumps(body,ensure_ascii=False,separators=(',',':')).encode()).hexdigest()
            event=json.loads(fixture.wire(rid).split(b'data: ',1)[1])
            event['response']['model']=row['model']
            wire=b'data: '+json.dumps(event).encode()+b'\n\n'
            g=fixture.gateway.Gateway({'model':row['model'],'effort':row['effort']},
                lambda raw,remaining:(200,'text/event-stream',iter((wire,))),clock=lambda:0)
            observer=pipe.PipeObserver(row['assignment_id'],row['model'],row['effort'],started_at=0,clock=lambda:0)
            observer.begin(1,request_hash)
            observer.feed(fixture.encoded(list(g.handle({'schema_version':1,'id':1,'body':body}))))
            run='fixture-run-'+str(index)
            m.begin_trial(row['assignment_id'],run,APPROVAL)
            m.before_request(row['assignment_id'],1,request_hash)
            m.ingest(row['assignment_id'],observer.receipt())
            m.ingest(row['assignment_id'],observer.receipt())
            m.finish_trial(row['assignment_id'],'completed',{**cleanup(),'run_id':run})
        state=m.snapshot()
        self.assertTrue(all(row['status']=='completed' for row in state['rows']))
        self.assertEqual(m.observation()['known_cost_microcredits'],10059600)
        self.assertFalse(m.observation()['budget_observation_allows_next'])
        self.assertFalse(m.observation()['execution_authorized'])
        self.assertEqual(state['model_calls_started'],0)
        self.assertTrue(all(row['objective'] is None and row['human_acceptance'] is None for row in state['rows']))
        self.assertEqual(set(self.directory.iterdir()),{self.directory/'results.json',self.directory/'results.lock'})

    def test_symlink_results_or_lock_never_overwrites_target(self):
        for name in ('results.json','results.lock'):
            with self.subTest(name=name),tempfile.TemporaryDirectory(prefix='relay-controller-results-test-') as tmp:
                target=Path(tmp)/'canary';target.write_bytes(b'UNCHANGED')
                (Path(tmp)/name).symlink_to(target)
                with self.assertRaises((ValueError,OSError)):
                    config=configuration()
                    manager.ResultsManager(Path(tmp),MANIFEST,TEMPLATE,budget_microcredits=48000000,
                        configuration=config,verified_configuration_sha256=digest(config))
                self.assertEqual(target.read_bytes(),b'UNCHANGED')

if __name__=='__main__':unittest.main()
