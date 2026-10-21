import copy
import hashlib
import json
from pathlib import Path
import unittest
from context_responses import ContextScript, canonical_catalog

HERE=Path(__file__).resolve().parent
class ContextResponses(unittest.TestCase):
    def setUp(self):
        # Synthetic input only; this is not an observed native SDK catalogue.
        self.tools=[{"type":"function","name":n} for n in ["list_mcp_resources","list_mcp_resource_templates","read_mcp_resource"]]+[{"type":"namespace","name":"mcp__finite","tools":[{"type":"function","name":n} for n in ["exec","read_file","read_output","write_file"]]}]
        self.hash=hashlib.sha256(canonical_catalog(self.tools)).hexdigest()
        self.script=ContextScript(HERE,self.hash)
        self.input=[{"type":"message","role":"user","content":[{"type":"input_text","text":"RTK-CONTEXT-AUTHORED-V1"}]}]
    def request(self):
        return {"model":"gpt-6-luna","tools":self.tools,"input":self.input}
    def reply(self,value,index):
        self.input.append({"type":"function_call_output","call_id":f"call_context_{index}","output":[{"type":"input_text","text":"Wall time: 0.0001 seconds\nOutput:"},{"type":"input_text","text":json.dumps(value)}]})
    def value(self,text="",code=0,stderr=""):
        return {"stdout":text,"stderr":stderr,"exit_code":code,"termination":"exited","truncated":False,"elapsed_ms":1}
    def test_catalog_mismatch_refuses_without_authored_response(self):
        bad=ContextScript(HERE,'0'*64).respond(self.request())
        self.assertTrue(bad['refused']);self.assertNotIn('sse',bad)
        self.assertEqual(bad['catalog_digest'],self.hash)
    def test_sequence_checks_exact_window_red_view_raw_and_single_exec(self):
        first=self.script.respond(self.request());self.assertFalse(first['refused']);self.assertIn('"namespace":"mcp__finite"',first['sse'])
        value=self.value('é\r\n');value['window']={'source_sha256':hashlib.sha256((HERE/'source.txt').read_bytes()).hexdigest(),'first_line':2,'returned_lines':1,'total_lines':3,'has_more':True}
        self.reply(value,0);self.assertFalse(self.script.respond(self.request())['refused'])
        red=self.value('RED é\r\n',1,'detail\n');raw='\x1b[31m'*80+'RED é\r\n'+'\x1b[0m'
        red['view']={'method':'terminal_sgr_v1','raw_handle':'c'*64,'raw_content_bytes':len(raw.encode())+7,'shown_content_bytes':len('RED é\r\n'.encode())+7}
        self.reply(red,1);reply=self.script.respond(self.request());self.assertFalse(reply['refused']);self.assertIn('c'*64,reply['sse'])
        self.reply(self.value(raw,1,'detail\n'),2);self.assertFalse(self.script.respond(self.request())['refused'])
        self.reply(self.value('1'),3);self.assertFalse(self.script.respond(self.request())['refused'])
        self.reply(self.value(),4);last=self.script.respond(self.request());self.assertFalse(last['refused']);self.assertEqual(last['tool_results'],5);self.assertIn('CONTEXT_AUTHORED_DONE',last['sse'])
    def test_truncated_result_refuses(self):
        self.script.respond(self.request());value=self.value('é\r\n');value['truncated']=True;self.reply(value,0)
        self.assertTrue(self.script.respond(self.request())['refused'])
    def test_changed_native_schema_and_unknown_results_refuse(self):
        self.tools[-1]['tools'][0]['extra']='changed'
        self.assertTrue(self.script.respond(self.request())['refused'])
        self.setUp();self.script.respond(self.request());self.reply(self.value(),99)
        self.assertTrue(self.script.respond(self.request())['refused'])
    def test_additional_tools_cannot_expand_the_catalog(self):
        self.input.append({'type':'additional_tools','tools':[{'name':'arbitrary'}]})
        self.assertTrue(self.script.respond(self.request())['refused'])
    def test_non_integer_exit_code_and_duplicate_json_refuse(self):
        self.script.respond(self.request());value=self.value('é\r\n');value['exit_code']=False;self.reply(value,0)
        self.assertTrue(self.script.respond(self.request())['refused'])
        self.setUp();self.script.respond(self.request());self.reply(self.value(),0)
        self.input[-1]['output'][1]['text']='{"exit_code":0,"exit_code":0}'
        self.assertTrue(self.script.respond(self.request())['refused'])


class StreamingShape(unittest.TestCase):
    def test_authored_calls_stream_arguments_and_keep_usage_unknown(self):
        tools=[{'name':n,'type':'function'} for n in ['list_mcp_resources','list_mcp_resource_templates','read_mcp_resource']]+[{'name':'mcp__finite','type':'namespace','tools':[{'name':n,'type':'function'} for n in ['exec','read_file','read_output','write_file']]}]
        script=ContextScript(HERE,hashlib.sha256(canonical_catalog(tools)).hexdigest())
        result=script.respond({'model':'gpt-6-luna','tools':tools,'input':[{'type':'message','content':'RTK-CONTEXT-AUTHORED-V1'}]})
        events=[json.loads(line[6:]) for line in result['sse'].splitlines() if line.startswith('data: ')]
        self.assertEqual([event['type'] for event in events],['response.created','response.output_item.added','response.function_call_arguments.delta','response.function_call_arguments.done','response.output_item.done','response.completed'])
        self.assertNotIn('usage',events[-1]['response'])


class FinalMessageShape(unittest.TestCase):
    def test_final_authored_message_starts_with_output_text_slot(self):
        script=ContextScript(HERE,'0'*64)
        events=[json.loads(line[6:]) for line in script.response(5).decode().splitlines() if line.startswith('data: ')]
        self.assertEqual(events[1]['item']['content'],[{'type':'output_text','text':'','annotations':[]}])

if __name__=='__main__':unittest.main()
