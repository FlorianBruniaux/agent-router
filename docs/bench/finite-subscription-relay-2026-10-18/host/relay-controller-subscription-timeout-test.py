"""Fake-clock regression tests; no credentials, network or actual waiting."""
import importlib.util,json,os
from pathlib import Path
import unittest
from unittest.mock import patch

ROOT=Path(__file__).resolve().parent
_spec=importlib.util.spec_from_file_location('timeout_launcher_helpers',ROOT/'relay-controller-subscription-launcher-test.py')
helpers=importlib.util.module_from_spec(_spec);_spec.loader.exec_module(helpers)
host=helpers.launcher

class PreparationTimeoutTests(unittest.TestCase):
    def setUp(self):
        self.fixture=helpers.LauncherTests('test_review_is_read_only_and_binds_all_24_budget_account_code_and_destination')
        self.fixture.setUp();self.addCleanup(self.fixture.doCleanups)
        self.plan=host.prepare(self.fixture.spec);self.binding=self.fixture.binding()
        self.epoch=1000.0;self.now=[self.epoch];self.channels=[];self.sends=[];self.auth_reads=[]
    def exercise(self,ready_at,after_ready=1,*,ready=True,request_before_ready=False):
        session=self.plan.manager.load_reviewed_session(host.PINS['relay-controller-session.py'])
        cls=session.PrivateRuntimeIO;original_init=cls.__init__;original_read=cls.read;original_pin=session.load_pinned
        incoming_read,incoming_write=os.pipe();outgoing_read,outgoing_write=os.pipe()
        body={'model':'gpt-6-luna','reasoning':{'effort':'xhigh'},'stream':True,'store':False,
            'input':[{'role':'user','content':'SYNTHETIC TIMEOUT TEST'}],
            'tools':json.loads((helpers.QUALIFIED/'native-catalog-adapter/expected-native-catalog.json').read_text())}
        frames=[{'schema_version':1,'kind':'runtime_ready','binding':self.binding},
            {'schema_version':1,'id':1,'body':body},
            {'schema_version':1,'kind':'runtime_cleanup','binding':self.binding,'cleanup':'namespace_terminated',
             'native_status':'completed','proof_sha256':'c'*64}]
        reads=[0]
        def initialise(channel,*args,**kwargs):
            original_init(channel,*args,**kwargs,clock=lambda:self.now[0]);self.channels.append(channel)
        def read(channel):
            index=reads[0];reads[0]+=1
            self.now[0]=self.epoch+(ready_at if index==0 else ready_at+after_ready+(index-1))
            frame=frames[1] if request_before_ready and index==0 else frames[index]
            if ready:os.write(incoming_write,host.canonical(frame)+b'\n')
            return original_read(channel)
        def pin(name):
            module=original_pin(name)
            if name=='subscription_gateway.py':
                def forbidden_auth(*args):self.auth_reads.append(True);raise AssertionError('credential_read_forbidden')
                def factory(auth_loader):
                    def send(raw,remaining):
                        channel=self.channels[0];self.sends.append((self.now[0],remaining,channel.deadline,channel.timeout))
                        response={'id':'synthetic_timeout_response','model':'gpt-6-luna','status':'completed','output':[],
                            'usage':{'input_tokens':400,'output_tokens':200,'total_tokens':600,'input_tokens_details':{'cached_tokens':0},'output_tokens_details':{'reasoning_tokens':0}}}
                        wire=b'data: '+json.dumps({'type':'response.completed','response':response}).encode()+b'\n\n'
                        return 200,'text/event-stream',iter((wire,))
                    return send
                module.host_auth=forbidden_auth;module.subscription_sender=factory
            return module
        session.load_pinned=pin
        try:
            with patch.object(cls,'__init__',initialise),patch.object(cls,'read',read),patch.object(self.plan.manager,'load_reviewed_session',return_value=session):
                return host.dispatch_subscription(self.plan,incoming_read,outgoing_write,self.binding)
        finally:
            for fd in (incoming_read,incoming_write,outgoing_read,outgoing_write):os.close(fd)
    def state(self):return json.loads((self.fixture.directory/'results.json').read_text())
    def test_ready_after_90_seconds_starts_with_only_60_seconds_for_requests(self):
        result=self.exercise(90)
        self.assertEqual(self.state()['rows'][0]['status'],'completed')
        self.assertEqual(len(self.sends),1);at,remaining,deadline,timeout=self.sends[0]
        self.assertEqual((at,deadline,timeout),(self.epoch+91,self.epoch+150,60));self.assertGreater(remaining,0);self.assertLessEqual(remaining,60)
        self.assertEqual(result['known_model_responses'],1);self.assertEqual(self.auth_reads,[])
    def test_no_ready_at_300_seconds_is_refused_without_attempt_or_provider(self):
        for ready_at in (300,301):
            with self.subTest(ready_at=ready_at):
                self.now[0]=self.epoch;self.channels.clear()
                with self.assertRaises(TimeoutError):self.exercise(ready_at,ready=False)
                state=self.state();self.assertTrue(all(r['status']=='not_run' for r in state['rows']))
                self.assertEqual(state['model_calls_started'],0);self.assertEqual(self.sends,[]);self.assertEqual(self.auth_reads,[])
                self.assertEqual(self.channels[0].deadline,self.epoch+300);self.assertEqual(self.channels[0].timeout,60)
    def test_exactly_60_seconds_after_ready_is_still_a_strict_timeout(self):
        with self.assertRaises(TimeoutError):self.exercise(90,after_ready=60)
        self.assertEqual(self.channels[0].deadline,self.epoch+150);self.assertEqual(self.channels[0].timeout,60)
        self.assertEqual(self.state()['rows'][0]['status'],'stopped');self.assertEqual(self.sends,[]);self.assertEqual(self.auth_reads,[])
    def test_request_before_ready_never_enters_provider_or_starts_a_trial(self):
        with self.assertRaises(ValueError):self.exercise(299,request_before_ready=True)
        self.assertTrue(all(r['status']=='not_run' for r in self.state()['rows']))
        self.assertEqual(self.sends,[]);self.assertEqual(self.auth_reads,[])

if __name__=='__main__':unittest.main()
