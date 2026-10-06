"""Offline behavioral tests; no native process or model provider needed."""
import copy
import json
import unittest
import probe_v5 as p


class CallbackTests(unittest.TestCase):
    def message(self):
        return {'id': 17, 'method': 'item/tool/call', 'params': {
            'threadId': 'thread', 'turnId': 'turn', 'callId': 'native-call',
            'tool': 'u02_registration_canary', 'namespace': None,
            'arguments': {'nonce': 'U02_CODE_MODE_NONCE'}}}

    def test_exact_callback_returns_fixed_output_once(self):
        gate = p.CallbackGate('thread', 'turn')
        answer = gate.respond(self.message())
        self.assertEqual(answer, {'id': 17, 'result': {'contentItems': [
            {'type': 'inputText', 'text': 'U02_CODE_MODE_CALLBACK_ACCEPTED'}], 'success': True}})
        self.assertEqual(gate.accepted[0]['call_id'], 'native-call')
        self.assertIsNone(gate.respond(self.message()))
        self.assertEqual(len(gate.accepted), 1)

    def test_wrong_identity_tool_arguments_or_namespace_is_refused(self):
        for field, value in [('threadId','other'), ('turnId','other'), ('callId',''),
                ('tool','unexpected'), ('arguments',{}), ('namespace','untrusted')]:
            with self.subTest(field=field):
                gate = p.CallbackGate('thread', 'turn'); message = self.message()
                message['params'][field] = value
                self.assertIsNone(gate.respond(message))
                self.assertEqual(gate.accepted, [])
        gate = p.CallbackGate('thread','turn'); message = self.message(); message['id'] = True
        self.assertIsNone(gate.respond(message))


class ProjectionTests(unittest.TestCase):
    def test_finds_nested_declaration_without_exporting_prompt(self):
        request = {'model':'gpt-6-astra','reasoning':{'effort':'high'},'input':[
            {'type':'additional_tools','tools':[{'type':'namespace','name':'functions','tools':[
                {'type':'custom','name':'exec','description':'private u02_registration_canary private'}]}]},
            {'type':'message','role':'user','content':'private prompt'}]}
        actual = p.project(request)
        self.assertEqual(actual['canary_pointers'], ['/input/0/tools/0/tools/0/description'])
        self.assertEqual(actual['exec_pointers'], ['/input/0/tools/0/tools/0'])
        self.assertNotIn('private', json.dumps(actual))

    def test_matching_output_needs_exact_call_and_fixed_text(self):
        request = {'input':[{'type':'custom_tool_call_output','call_id':'call_u02_exec',
            'output':[{'type':'input_text','text':'U02_CODE_MODE_CALLBACK_ACCEPTED'}]}]}
        self.assertTrue(p.project(request)['matching_output'])
        for field,value in [('call_id','other'),('type','function_call_output'),('output','unrelated')]:
            mutated=copy.deepcopy(request); mutated['input'][0][field]=value
            self.assertFalse(p.project(mutated)['matching_output'])


class StreamTests(unittest.TestCase):
    def test_script_contains_only_fixed_custom_call(self):
        frames=[json.loads(x.split('data: ',1)[1]) for x in p.call_sse().decode().strip().split('\n\n')]
        call=next(x['item'] for x in frames if x['type']=='response.output_item.done')
        self.assertEqual(call.get('type'),'custom_tool_call')
        self.assertEqual(call['name'],'exec')
        self.assertEqual(call['namespace'],'functions')
        self.assertEqual(call['call_id'],'call_u02_exec')
        self.assertEqual(call['input'],'text(await tools.u02_registration_canary({nonce: "U02_CODE_MODE_NONCE"}));')


class VerdictTests(unittest.TestCase):
    def case(self):
        return {'mode':'enabled','thread_id':'t','turn_id':'u','callback_count':1,
            'callback_refused_count':0,'callback_item_correlated':True,
            'turn_started_correlated':True,'turn_completed_correlated':True,
            'final_output_correlated':True,'invalid_effort_rejected_before_provider':True,
            'cleanup_complete':True,'temporary_state_removed':True,'owned_process_group_gone':True,
            'provider_request_count':2,'provider_accepted_connections':2,
            'provider_rejected_requests':0,'provider_rejected_connections':0,
            'provider_requests':[
                {'model':'gpt-6-astra','effort':'high','canary_pointers':['/input/0/tools/0/tools/0/description'],
                 'exec_pointers':['/input/0/tools/0/tools/0'],'matching_output':False},
                {'model':'gpt-6-astra','effort':'high','matching_output':True}]}

    def test_complete_roundtrip_passes_and_missing_evidence_fails(self):
        self.assertTrue(p.verdict(self.case()))
        for field in ('callback_item_correlated','turn_started_correlated','turn_completed_correlated',
                'final_output_correlated','invalid_effort_rejected_before_provider','cleanup_complete','temporary_state_removed','owned_process_group_gone'):
            case=self.case(); case[field]=False
            self.assertFalse(p.verdict(case),field)
        for field in ('provider_request_count','provider_accepted_connections','callback_count'):
            case=self.case(); case[field]+=1
            self.assertFalse(p.verdict(case),field)
        case=self.case(); case['provider_requests'][1]['matching_output']=False
        self.assertFalse(p.verdict(case))

    def test_disabled_requires_fresh_baseline_without_canary_or_callback(self):
        case=self.case(); case.update(mode='disabled',callback_count=0,provider_request_count=1,
            provider_accepted_connections=1,callback_item_correlated=False)
        case['provider_requests']=[{'model':'gpt-6.1-sol','effort':'low','canary_pointers':[],
            'exec_pointers':['/input/0/tools/0/tools/0'],'matching_output':False}]
        self.assertTrue(p.verdict(case))
        case['provider_requests'][0]['canary_pointers']=['/input/0/tools/0/description']
        self.assertFalse(p.verdict(case))

class ExactOutputRegressionTests(unittest.TestCase):
    def test_marker_in_error_string_or_wrong_content_type_is_not_output(self):
        for output in ('error: U02_CODE_MODE_CALLBACK_ACCEPTED did not happen',
                [{'type':'input_text','text':'wrong U02_CODE_MODE_CALLBACK_ACCEPTED'}],
                [{'type':'other','text':'U02_CODE_MODE_CALLBACK_ACCEPTED'}]):
            self.assertFalse(p.project({'input':[{'type':'custom_tool_call_output',
                'call_id':'call_u02_exec','output':output}]})['matching_output'])


class ProviderBoundaryTests(unittest.TestCase):
    def setUp(self):
        import threading
        from pathlib import Path
        self.h = p.load_helper(Path(p.__file__).resolve().parent.parent/'egress-admission'/'codex_appserver_local.py')
        self.server=self.h.LocalProvider()
        self.server.RequestHandlerClass=p.handler_class(self.h)
        self.server.projections=[];self.server.mode='enabled';self.server.request_limit=2;self.server.raw_dir=None
        self.listener=threading.Thread(target=self.server.serve_forever,daemon=True);self.listener.start()

    def tearDown(self):
        self.server.shutdown();self.server.server_close();self.listener.join(3)
        self.assertFalse(self.listener.is_alive())

    def contact(self, body='{}', path='/v1/responses', method='POST', headers=None):
        import http.client
        connection=http.client.HTTPConnection('127.0.0.1',self.server.server_address[1],timeout=3)
        try:
            connection.request(method,path,body,headers or {})
            response=connection.getresponse();data=response.read()
            return response.status,data
        finally:connection.close()

    def test_third_physical_request_is_refused(self):
        self.assertEqual(self.contact()[0],200)
        self.assertEqual(self.contact()[0],200)
        self.assertEqual(self.contact()[0],429)
        self.assertEqual(self.server.request_count,3)
        self.assertEqual(self.server.rejected_requests,1)
        self.assertEqual(len(self.server.projections),2)

    def test_malformed_wrong_route_method_and_oversized_are_counted(self):
        cases=[dict(body='{'),dict(path='/wrong'),dict(method='GET'),
            dict(body='',headers={'Content-Length':str(self.h.MAX_REQUEST_BYTES+1)})]
        for args in cases:
            # Each is independent: rejected traffic consumes the request budget.
            with self.server.lock:self.server.request_count=0
            self.assertGreaterEqual(self.contact(**args)[0],400)
        self.assertEqual(self.server.rejected_requests,4)
        self.assertEqual(self.server.projections,[])

class NativeItemTests(unittest.TestCase):
    def test_item_must_match_callback_and_confirm_exact_success(self):
        callback={'call_id':'native-call'}
        item={'type':'dynamicToolCall','id':'native-call','tool':'u02_registration_canary',
            'namespace':None,'arguments':{'nonce':'U02_CODE_MODE_NONCE'},'status':'completed',
            'success':True,'contentItems':[{'type':'inputText','text':'U02_CODE_MODE_CALLBACK_ACCEPTED'}]}
        self.assertTrue(p.callback_item_matches(item,callback))
        for key,value in [('id','other'),('success',False),('status','failed'),('contentItems',[]),
                ('arguments',{}),('namespace','other'),('tool','other')]:
            changed=copy.deepcopy(item);changed[key]=value
            self.assertFalse(p.callback_item_matches(changed,callback),key)

class ProcessCleanupTests(unittest.TestCase):
    def test_exited_group_is_gone_and_live_group_is_not(self):
        import subprocess,sys
        proc=subprocess.Popen([sys.executable,'-c','import time; time.sleep(5)'],start_new_session=True)
        try:
            self.assertFalse(p.wait_group_gone(proc.pid,0.02))
        finally:
            proc.kill();proc.wait(timeout=2)
        self.assertTrue(p.wait_group_gone(proc.pid,0.2))

class RefusalCorrelationTests(unittest.TestCase):
    def refusal_case(self):
        case=VerdictTests().case()
        case.update(mode='refused',callback_count=0,callback_refused_count=1,
            provider_request_count=1,provider_accepted_connections=1,refused_before_fixed_output=True)
        case['provider_requests']=case['provider_requests'][:1]
        return case

    def test_generic_rejection_cannot_pass_expected_nonce_negative_control(self):
        self.assertFalse(p.verdict(self.refusal_case()))

    def test_only_exact_correlated_wrong_nonce_matches_negative_control(self):
        message=CallbackTests().message()
        message['params']['arguments']={'nonce':'U02_UNEXPECTED_NONCE'}
        self.assertTrue(p.expected_nonce_rejection(message,'thread','turn'))
        case=self.refusal_case();case['expected_nonce_rejection_correlated']=True
        self.assertTrue(p.verdict(case))
        for field,value in [('threadId','other'),('turnId','other'),('tool','other'),
                ('callId',''),('namespace','other'),('arguments',{'nonce':'U02_CODE_MODE_NONCE'})]:
            changed=copy.deepcopy(message);changed['params'][field]=value
            correlated=p.expected_nonce_rejection(changed,'thread','turn')
            self.assertFalse(correlated,field)
            case['expected_nonce_rejection_correlated']=correlated
            self.assertFalse(p.verdict(case),field)

class GroupObserverTests(unittest.TestCase):
    def test_complete_numeric_inventory_distinguishes_absent_and_present(self):
        actual=p.parse_group_inventory(b'  123\n 456\n123\n',0)
        self.assertEqual(actual,{'status':'complete','groups':{123,456}})

    def test_failed_empty_truncated_malformed_or_oversized_inventory_is_unknown(self):
        cases=[(b'123\n',1),(b'',0),(b'123',0),(b'123\nbad\n',0),
            (b'123\n\n',0),(b'-1\n',0),(b'1\n'*150000,0)]
        for output,code in cases:
            result=p.parse_group_inventory(output,code)
            self.assertNotEqual(result.get('status'),'complete')
            self.assertNotIn('groups',result)

    def test_permission_denial_uses_only_complete_secondary_observation(self):
        from unittest.mock import patch
        for observed,want in [({'status':'complete','groups':{123}},False),
                ({'status':'complete','groups':{456}},True),({'status':'nonzero_exit'},False),
                ({'status':'malformed'},False),({'status':'timed_out'},False)]:
            with patch.object(p.os,'killpg',side_effect=PermissionError), patch.object(p,'numeric_group_inventory',return_value=observed):
                evidence={}
                self.assertEqual(p.wait_group_gone(123,0.02,evidence),want)
                self.assertEqual(evidence.get('group_signal_observation'),'permission_error')

    def test_existing_signal_group_does_not_become_absent_via_fallback(self):
        from unittest.mock import patch
        with patch.object(p.os,'killpg',return_value=None), patch.object(p,'numeric_group_inventory') as inventory:
            evidence={}
            self.assertFalse(p.wait_group_gone(123,0.01,evidence))
            inventory.assert_not_called()
            self.assertEqual(evidence.get('group_signal_observation'),'present')

    def test_inventory_reader_stops_at_byte_and_time_limits(self):
        import sys
        oversized=p.numeric_group_inventory(1,command=[sys.executable,'-c','import sys;sys.stdout.write("1\\n"*150000)'])
        self.assertEqual(oversized['status'],'oversized')
        stalled=p.numeric_group_inventory(0.1,command=[sys.executable,'-c','import time;time.sleep(5)'])
        self.assertEqual(stalled['status'],'timed_out')

if __name__ == '__main__': unittest.main()
